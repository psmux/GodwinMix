// A source's settings, in the drawer.
//
// Its own module so that none of it is on the page until somebody presses a
// gear: the form generator, the device list and the rules about what
// `source.set` will take all arrive with the first click.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { shell } from "../../shell/shell.js";
import { SchemaForm } from "../../client/schema-form.js";
import { SOURCE_KINDS, kindOfUri, discoverDevices } from "../../client/kinds.js";
import { schemaForSource, easeSchema, unease } from "../../client/devices.js";
import { settableOnly, setRequest } from "./setreq.js";
import { nameOf, setLocal } from "./local.js";

/** @param {object} panel the Sources panel, for its client and its redraw */
export async function openSourceDrawer(panel, source) {
  const client = panel.client;
  const id = source.id;
  const kind = SOURCE_KINDS.find((k) => k.id === kindOfUri(source.uri)) || SOURCE_KINDS[0];
  // The plugin's own form for this kind of source, with the box that picks
  // a device offered as a list of the devices there are. It used to ask
  // `plugin.describe` for an instance, which that method does not take, so
  // every source got the built in form for its address, and a camera has
  // no address to tell it by.
  let schema = (await schemaForSource(client, source).catch(() => null)) || kind.schema;
  const found = await discoverDevices(client, 1500).catch(() => []);
  schema = easeSchema(schema, source.type || "", found, {});
  const form = new SchemaForm(settableOnly(schema), { name: nameOf(source) });
  // What the form says before anybody has touched it. Only what differs
  // from this is sent. The mixer does not publish a source's settings, so
  // the boxes open at their defaults, and sending every one of them would
  // quietly put a second camera back to the first and its size back to auto.
  const untouched = unease(form.read());
  const changed = () => {
    const now = unease(form.read());
    const out = {};
    for (const [key, value] of Object.entries(now)) {
      if (JSON.stringify(value) !== JSON.stringify(untouched[key])) out[key] = value;
    }
    return out;
  };
  const apply = el("button.btn.primary", {
    text: "Apply",
    onclick: async () => {
      const wanted = changed();
      if (!Object.keys(wanted).length) {
        toast({ text: "Nothing was changed." });
        return;
      }
      try {
        await client.call("source.set", setRequest(id, wanted));
        toast({ text: "Saved." });
      } catch (e) {
        if (e.code === -32601) {
          setLocal(id, { name: form.read().name });
          toast({ kind: "warning", text: "This mixer cannot save settings on a source yet, so the name is kept on this device only." });
          panel.render(client.state);
        } else {
          errorToast(e, "Save");
        }
      }
    },
  });
  shell.drawer(
    el("div.pad.col", {}, [
      el("div.row", {}, [el("strong.grow", { text: nameOf(source) }), el("button.btn.icon", { text: "×", onclick: () => shell.drawer(null) })]),
      addressRow(client, source),
      form.el,
      el("div.row", {}, [apply]),
    ])
  );
}

/**
 * The address, and a way to change it.
 *
 * `source.set` will not take a new `uri` (the mixer builds a source around
 * its address), so a new address is the same source removed and added again
 * under the same id and name. Scene items point at the id, so they keep
 * pointing at it. This used to be a sentence telling the person to do those
 * two steps by hand, which lost the name and every placement on the way.
 * A plugin's source has no address to edit, so it shows the address only.
 */
function addressRow(client, source) {
  const shown = el("div.sm.dim", { text: source.uri, title: source.uri });
  if (source.type && source.type.includes("/")) return shown;
  const input = el("input", { type: "text", value: source.uri, "aria-label": "Address" });
  const button = el("button.btn", { text: "Change the address" });
  button.onclick = async () => {
    const uri = input.value.trim();
    if (!uri || uri === source.uri) return toast({ text: "That is the address it already has." });
    const onAir = client.state.program === source.id;
    const yes = await confirmModal(
      `Changing the address takes ${nameOf(source)} out and puts it back at the new one, ` +
        `with the same name and the same places in scenes.` +
        (onAir ? " It is on air, so the programme shows the slate for that moment." : ""),
      "Change it"
    );
    if (!yes) return;
    button.disabled = true;
    try {
      await readdress(client, source, uri);
      toast({ text: `${nameOf(source)} now plays ${uri}.` });
      shell.drawer(null);
    } catch (e) {
      errorToast(e, "Change the address");
    } finally {
      button.disabled = false;
    }
  };
  return el("div.col", {}, [el("div.row", {}, [input, button])]);
}

/** Remove and add again under the same id. A refused add puts the old one back. */
export async function readdress(client, source, uri) {
  const name = source.name || source.id;
  await client.call("source.remove", { id: source.id });
  try {
    await client.call("source.add", { id: source.id, name, uri });
  } catch (e) {
    await client.call("source.add", { id: source.id, name, uri: source.uri }).catch(() => {});
    throw e;
  }
}
