// Edit a graphic's words and colours in place, watching the picture change.
//
// Each change asks `gallery.preview` with the values typed so far, a
// quarter of a second after the typing stops, so nothing is drawn per key.
// Save keeps the values with the item (`gallery.edit`); a shipped graphic is
// copied first, since the pack is read only. Anything on air showing the
// item gets the new words at once through `source.set`.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { input } from "../sources/graphic-pick.js";
import { ordered, colourInput } from "../sources/graphic-fields.js";

const SETTLE_MS = 250;

export function editGraphic(client, item, onSaved) {
  const values = { ...(item.values || {}) };
  const shown = (f) => (values[f.name] !== undefined ? String(values[f.name]) : f.type === "color" ? colourInput(f.default) : String(f.default ?? ""));
  const pic = el("img.gx-edit-pic", { alt: `${item.name} with these words` });
  let timer = null;
  const redraw = () => {
    clearTimeout(timer);
    timer = setTimeout(async () => {
      try {
        const p = await client.call("gallery.preview", { id: item.id, width: 960, values });
        pic.src = `data:image/jpeg;base64,${p.image}`;
      } catch (e) {
        errorToast(e, "Preview");
      }
    }, SETTLE_MS);
  };
  const rows = ordered(item.fields).map((f) =>
    el("label.col.sm", {}, [el("span.dim", { text: f.label || f.name }), input(f, shown(f), (v) => ((values[f.name] = v), redraw()))])
  );
  const save = el("button.btn.primary", { text: item.origin === "shipped" ? "Save as a copy" : "Save" });
  const m = modal({
    title: `Edit ${item.name}`,
    wide: true,
    body: el("div.gx-edit", {}, [el("div.gx-edit-stage", {}, [pic]), el("div.col.gx-edit-fields", {}, rows)]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), save],
  });
  save.onclick = async () => {
    save.disabled = true;
    try {
      const target = item.origin === "shipped" ? (await client.call("gallery.duplicate", { id: item.id })).item : item;
      const saved = await client.call("gallery.edit", { id: target.id, values });
      for (const source of saved.item.placed || []) {
        await client.call("source.set", { id: source, params: { fields: values } }).catch(() => {});
      }
      toast({ text: `${saved.item.name} saved.` });
      m.close();
      if (onSaved) onSaved(saved.item);
    } catch (e) {
      errorToast(e, `Save ${item.name}`);
      save.disabled = false;
    }
  };
  redraw();
  return m;
}
