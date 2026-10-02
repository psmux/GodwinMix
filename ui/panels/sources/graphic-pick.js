// Graphics in Add a source: the template pack and the SVG templates in the
// media library, one row each, and the short form that asks for a graphic's
// words before it is added.
//
// The rows come from `template.list`, asked for the first time somebody opens
// the Graphics category or searches, never before. Picking one opens a form
// with the template's fields filled with their defaults; Add sends only what
// was changed, so colours left alone keep following the station's brand.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { canvasOf } from "./text-presets.js";
import { addRequest, changedOnly, colourInput, ordered, placementFor } from "./graphic-fields.js";

/** Every template the mixer can draw. Throws what the mixer said. */
export async function loadTemplates(client) {
  const listing = await client.call("template.list", {});
  return (listing && listing.templates) || [];
}

/** One picker row for one template. Its button opens the fields form. */
export function graphicEntry(client, template, opts) {
  return {
    icon: "graphic",
    name: template.title || template.name,
    note: template.description || template.name,
    title: template.uri,
    label: "Choose",
    run: () => askFields(client, template, opts),
    added: () => false,
  };
}

/** The form a pick opens: each field, filled with its default. */
export function askFields(client, template, opts = {}) {
  const shown = {};
  const values = {};
  const rows = ordered(template.fields).map((field) => {
    shown[field.name] = field.type === "color" ? colourInput(field.default) : String(field.default ?? "");
    values[field.name] = shown[field.name];
    return el("label.col.sm", {}, [el("span.dim", { text: field.label || field.name }), input(field, shown[field.name], (v) => (values[field.name] = v))]);
  });
  const note = el("p.sm.dim", { text: "The words can be changed on air afterwards, in the graphic's settings." });
  const add = el("button.btn.primary", { text: "Add" });
  const m = modal({
    title: `Add ${template.title || template.name}`,
    body: el("div.col.graphic-fields", {}, [...rows, note]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), add],
  });
  add.onclick = async () => {
    add.disabled = true;
    try {
      const answer = await client.call("source.add", addRequest(template, changedOnly(shown, values)));
      const placement = placementFor(await canvasOf(client));
      if (opts.onAdded) await opts.onAdded({ ...answer, placement });
      toast({ text: `${template.title || template.name} added. Its fields are in its settings.` });
      m.close();
    } catch (e) {
      errorToast(e, `Add ${template.title || template.name}`);
      add.disabled = false;
    }
  };
  return m;
}

/** A text box or a colour picker, calling `set` with what it holds. */
export function input(field, value, set) {
  if (field.type === "color") {
    const pick = el("input", { type: "color", value, "aria-label": field.label || field.name });
    pick.oninput = () => set(pick.value);
    return pick;
  }
  const box = el("input", { type: "text", value, "aria-label": field.label || field.name });
  box.oninput = () => set(box.value);
  return box;
}
