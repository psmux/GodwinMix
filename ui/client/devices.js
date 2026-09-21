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
    // `name` stays in: an NDI sender is picked by its name, and a camera's
    // candidate carries none, so nothing of a camera's is caught by it.
    (key) => key !== "label" && key !== "sizes" && !Array.isArray(props[key].enum) && mine.some((c) => c.params[key] !== undefined && c.params[key] !== "")
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

/**
 * Show the word a nought stands for.
 *
 * A plugin writes `"x-gmx-zero": "Automatic"` on a number whose 0 means
 * something: let the camera choose, off, wait for ever, any free port. The
 * box then opens empty with that word in it, where it used to open on a bare
 * 0 that a person had to read the small print to understand. Both form
 * generators already show a schema's first example as the hint in an empty
 * box, and both leave an empty number out of what they send, so the plugin
 * falls back to its own default, which is that same 0.
 */
export function zeroWords(schema) {
  for (const prop of Object.values((schema && schema.properties) || {})) {
    if (!prop || typeof prop["x-gmx-zero"] !== "string" || Array.isArray(prop.enum)) continue;
    prop.examples = [prop["x-gmx-zero"]];
    if (prop.default === 0) delete prop.default;
  }
  return schema;
}

export function foldAdvanced(schema) {
  zeroWords(schema);
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

/** Frame rates worth offering. Anything else is typed under Advanced. */
const RATES = [15, 24, 25, 30, 50, 60];

/**
 * A plugin's settings schema as a person should meet it.
 *
 * One function, because there are three places a source's settings are asked
 * for (the add form, the settings drawer, the composer's inspector) and they
 * had drifted: the first offered a list of cameras while the third still asked
 * for "the id from `list_cameras`". In order: devices and their sizes become
 * lists, a frame rate of 0 reads Automatic, a size nobody listed can still be
 * typed under Advanced, and whatever the plugin did not group is folded.
 */
export function easeSchema(schema, kindId, candidates, current) {
  let out = withDeviceChoices(JSON.parse(JSON.stringify(schema || {})), kindId, candidates, current || {});
  const props = out.properties || {};
  const rate = props.framerate || props.fps;
  if (rate && (rate.type === "integer" || rate.type === "number") && !Array.isArray(rate.enum)) {
    const key = props.framerate ? "framerate" : "fps";
    const was = current && Number(current[key]);
    const values = [0].concat(RATES);
    if (was && !values.includes(was)) values.push(was);
    // 0 is the plugin's own word for "let the device choose", and a box with a
    // nought in it says nothing of the kind.
    props[key] = Object.assign({}, rate, {
      enum: values,
      "x-gmx-labels": values.map((v) => (v === 0 ? "Automatic" : `${v} frames a second`)),
      description: "Automatic takes what the device prefers.",
    });
  }
  if (props.resolution && Array.isArray(props.resolution.enum) && !props.resolution_custom) {
    props.resolution_custom = {
      type: "string",
      title: "Custom size",
      description: "A size that is not in the list, as WIDTHxHEIGHT. Filled in, it is used in place of the list.",
      examples: ["1600x900"],
      "x-gmx-group": "Advanced",
    };
  }
  out.properties = props;
  return foldAdvanced(out);
}

/** The values of an eased form, as the plugin expects them. */
export function unease(values) {
  const out = Object.assign({}, values);
  const custom = String(out.resolution_custom || "").trim();
  if (custom) out.resolution = custom;
  delete out.resolution_custom;
  return out;
}

/** The same schema for the designer kit, which names a choice with `oneOf`. */
export function forKit(schema) {
  const out = JSON.parse(JSON.stringify(schema || {}));
  for (const prop of Object.values(out.properties || {})) {
    const labels = prop["x-gmx-labels"];
    if (!Array.isArray(prop.enum) || !Array.isArray(labels)) continue;
    prop.oneOf = prop.enum.map((value, i) => ({ const: value, title: String(labels[i] ?? value) }));
    delete prop.enum;
    delete prop["x-gmx-labels"];
  }
  return out;
}
