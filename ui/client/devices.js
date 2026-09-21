// Telling one device from another, and one address from another.
//
// Out of `kinds.js`, which every page loads, because only the source chooser
// and the settings drawer ask these questions and both are fetched on demand.
// The page has a byte budget, and this is five kilobytes it never needed.

import { addRequestFor, listPlugins } from "./kinds.js";

/**
 * Whether a source's published address is the address we would add.
 *
 * The core cuts everything after the host off an address before it publishes
 * it, because that is where a stream key lives, so `test://smpte` comes back
 * as `test://smpte/…` and an exact comparison never matches: the picker added
 * a second colour bars every time it was asked for the one the mixer had. An
 * address that is only a scheme and a host loses nothing to the cut, so the
 * cut form identifies it. One with a path does not, and two files would both
 * read `file:///…`, so those only ever match exactly.
 */
export function sameAddress(published, wanted) {
  const have = String(published || "").trim().toLowerCase();
  const want = String(wanted || "").trim().toLowerCase();
  if (!have || !want) return false;
  if (have === want) return true;
  const bare = /^([a-z][a-z0-9+.-]*):\/\/([^/@]+)\/?$/.exec(want);
  return !!bare && have === `${bare[1]}://${bare[2]}/…`;
}

/**
 * Whether this mixer already has the thing a candidate offers.
 *
 * A source record carries an id, a name and a URI and nothing else, and every
 * camera on a machine shares the one URI, so the name is what tells two of
 * them apart. It is the device's own name, because that is what the picker
 * adds it under.
 */
export function alreadyAdded(sources, candidate) {
  const want = String((candidate && candidate.name) || "").trim().toLowerCase();
  const uri = String(addRequestFor(candidate).uri || "").toLowerCase();
  const typed = String((candidate && (candidate.type || candidate.kind)) || "").toLowerCase();
  return (sources || []).some((s) => {
    const name = String(s.name || "").trim().toLowerCase();
    if (want && name === want) return true;
    // A URI that is only the type id names the kind, not this device, so it
    // proves nothing on its own.
    return !!uri && uri !== typed && sameAddress(s.uri, uri);
  });
}

/**
 * A kind's schema with the box that picks a device turned into a choice of
 * the devices this machine has: a camera, a microphone, a monitor.
 *
 * The box is titled "Camera" and sits at the top of the form, and what it
 * wants is an id, a name exactly as the operating system spells it, or a
 * number. Somebody who typed a name of their own there was refused by the
 * plugin after the form had gone. What `device.discover` found is what can be
 * chosen, by name, with the first one found as the empty choice it always
 * was. Nothing found, or nothing of this kind, leaves the box as it is, so a
 * device the monitor cannot see can still be typed in.
 */
export function withDeviceChoices(schema, kindId, candidates, current) {
  const props = schema && schema.properties;
  if (!props) return schema;
  const mine = (candidates || []).filter((c) => (c.type || c.kind) === kindId && c.params);
  if (!mine.length) return schema;
  // Whatever a candidate carries that the form also asks for is the thing that
  // picks the device: `device` for a camera or a microphone, `monitor` for a
  // screen. Not the label, which is a name for the person and not a choice.
  const keys = Object.keys(props).filter(
    (key) => key !== "label" && key !== "name" && key !== "sizes" && !Array.isArray(props[key].enum) && mine.some((c) => c.params[key] !== undefined && c.params[key] !== "")
  );
  if (!keys.length) return schema;
  const next = Object.assign({}, props);
  for (const key of keys) {
    const having = mine.filter((c) => c.params[key] !== undefined && c.params[key] !== "");
    const text = (props[key].type || "string") === "string";
    // A text setting has always taken empty to mean the first one found, so
    // that stays on offer. A number has no empty, and its first is its 0.
    const values = (text ? [""] : []).concat(having.map((c) => c.params[key]));
    const labels = (text ? ["The first one found"] : []).concat(having.map((c) => c.name || String(c.params[key])));
    const was = current && typeof current === "object" ? current[key] : key === "device" ? current : undefined;
    if (was !== undefined && was !== "" && !values.includes(was)) {
      values.push(was);
      labels.push(String(was));
    }
    // The schema's own words are about what to type: an id, a name, a number
    // counting from 0. Under a list they are instructions for a box that is
    // no longer there.
    const description = text ? "Pick one. The first one found is whichever the system lists first." : "Pick one.";
    next[key] = Object.assign({}, props[key], { enum: values, "x-gmx-labels": labels, description });
  }
  // The sizes a camera offers belong to the resolution list, not to a box of
  // their own, so they are read off the chosen device here.
  const resolution = next.resolution;
  const sized = mine.filter((c) => Array.isArray(c.params.sizes) && c.params.sizes.length > 0);
  if (resolution && (resolution.type || "string") === "string" && !Array.isArray(resolution.enum) && sized.length > 0) {
    const wanted = current && typeof current === "object" ? current.device : current;
    const chosen = sized.find((c) => c.params.device === wanted) || sized[0];
    const sizes = chosen.params.sizes;
    const picked = [""].concat(sizes);
    const names = ["Automatic (recommended)"].concat(
      sizes.map((size) => {
        const parts = String(size).split("x");
        const width = Number(parts[0]);
        const height = Number(parts[1]);
        const shape = width > height ? "wide" : width < height ? "tall" : "square";
        return `${parts[0]} x ${parts[1]}, ${shape}`;
      })
    );
    const kept = current && typeof current === "object" ? current.resolution : undefined;
    if (typeof kept === "string" && kept !== "" && !picked.includes(kept)) {
      picked.push(kept);
      names.push(kept);
    }
    // Automatic picks for the person, so the words say what it will do.
    next.resolution = Object.assign({}, resolution, {
      enum: picked,
      "x-gmx-labels": names,
      description: "Automatic takes the wide size closest to the canvas.",
    });
    // The list belongs on the form, not inside the Advanced fold.
    delete next.resolution["x-gmx-group"];
  }
  return Object.assign({}, schema, { properties: next });
}

/**
 * The settings schema of a source that already exists, or null.
 *
 * Found by the type the core publishes for it, through the plugin that
 * provides that type. The address cannot stand in: a camera's is cut down to
 * an ellipsis, and its settings used to open as the form of whatever kind came
 * first in the table, with boxes for a file path.
 */
export async function schemaForSource(client, source) {
  const type = String((source && source.type) || "");
  if (!type) return null;
  const plugins = await listPlugins(client);
  const owner = plugins.find((p) =>
    (p.provides || []).some((entry) => {
      const id = typeof entry === "string" ? entry : (entry && (entry.id || entry.type)) || "";
      return id === type || `${p.name}/${id}` === type;
    })
  );
  if (!owner) return null;
  const schema = await provideSchema(client, owner.name, type);
  return Object.keys(schema.properties || {}).length > 1 ? schema : null;
}

/**
 * What a first visit needs is named here. Everything else folds away.
 *
 * A plugin says which of its settings are advanced with `x-gmx-group`, and
 * one that says so is left exactly as it is. This is for the ones that do
 * not: an older copy of a first party plugin, or somebody else's. Without it
 * "Capture element", a list of GStreamer element names, stood on the camera
 * form beside the camera itself.
 */
const ESSENTIAL = /^(name|label|device|monitor|window|screen|display_index|uri|url|address|host|port|path|file|folder|key|stream_key|passphrase|password|text|title|message|mode|app|channel|source|input|output|resolution)$/;

export function foldAdvanced(schema) {
  const props = (schema && schema.properties) || {};
  const declared = Object.values(props).some((p) => p && p["x-gmx-group"]);
  if (declared) return schema;
  const required = new Set(schema.required || []);
  for (const [key, prop] of Object.entries(props)) {
    if (!prop || required.has(key) || ESSENTIAL.test(key)) continue;
    props[key] = Object.assign({}, prop, { "x-gmx-group": "Advanced" });
  }
  return schema;
}

/** A provide's settings schema, with a Name field the core always takes. */
export async function provideSchema(client, pluginName, id) {
  let found = null;
  try {
    const described = await client.call("plugin.describe", { id: pluginName });
    found = (described && described.schemas && described.schemas[id]) || null;
  } catch {
    /* an older core, or a plugin that went away between two calls */
  }
  const schema = found && typeof found === "object" ? JSON.parse(JSON.stringify(found)) : {};
  schema.type = "object";
  schema.properties = schema.properties || {};
  if (!schema.properties.name) schema.properties.name = { type: "string", title: "Name" };
  // One box for what to call it, in plain view and marked optional. A plugin's
  // own `label` asks the same question a second time, so it is taken off the
  // form and filled from the name when the source is added: see `openForm`.
  schema.properties.name = Object.assign({}, schema.properties.name, {
    title: "Name (optional)",
    description: "What to call it on its tile. Left empty, it takes the device's own name.",
  });
  if (schema.properties.label) {
    delete schema.properties.label;
    schema["x-gmx-name-is-label"] = true;
  }
  return foldAdvanced(schema);
}
