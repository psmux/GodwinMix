// A source's settings, in the drawer.
//
// Its own module so that none of it is on the page until somebody presses a
// gear: the form generator, the device list and the rules about what
// `source.set` will take all arrive with the first click.

import { el } from "../../shell/dom.js";
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
      el("div.sm.dim", { text: source.uri, title: source.uri }),
      el("div.sm.dim", {
        text: "The address is fixed once a source exists. To point it somewhere else, remove this source and add it again.",
      }),
      form.el,
      el("div.row", {}, [apply]),
    ])
  );
}
