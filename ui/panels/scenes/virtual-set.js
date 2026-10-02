// The Virtual set dialog: a presenter in front of a green or blue screen,
// put into a designed studio in one step.
//
// It asks three things: the picture behind (from the media library or a
// source), the camera, and an optional picture in front such as a desk. The
// one call it makes is `scene.virtual_set`, the same an agent makes, which
// turns library pictures into sources, guesses the key colour from the camera
// and builds the scene from the `virtual-set` layout. Fetched on open, never
// before.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";

/** What a picker offers: library files first, then sources. */
export function choices(media, sources, opts = {}) {
  const out = [];
  if (opts.none) out.push({ value: "", label: opts.none });
  for (const item of media || []) out.push({ value: item.name, label: `${item.name} (library)` });
  for (const s of sources || []) out.push({ value: s.id, label: s.name && s.name !== s.id ? `${s.name} (${s.id})` : s.id });
  return out;
}

/** The request the dialog sends, from what is picked. */
export function request(picked) {
  const body = { background: picked.background, presenter: picked.presenter };
  if (picked.foreground) body.foreground = picked.foreground;
  if (picked.name && picked.name.trim()) body.name = picked.name.trim();
  return body;
}

function select(label, options) {
  const box = el("select", { "aria-label": label });
  for (const o of options) box.appendChild(el("option", { value: o.value, text: o.label }));
  return box;
}

function field(label, control, hint) {
  return el("label.field", {}, [el("span.lbl", { text: label }), control, hint ? el("span.sm.faint", { text: hint }) : null].filter(Boolean));
}

/**
 * @param {{client, scenes, onMade?: (answer: object) => void}} opts
 */
export async function openVirtualSet(opts) {
  const { client } = opts;
  let media = [];
  try {
    media = ((await client.call("media.list", {})) || {}).items || [];
  } catch {
    // A core with no library still has sources to choose from.
  }
  const sources = client.state.sources || [];
  const background = select("Background", choices(media, sources));
  const presenter = select("Presenter", choices([], sources));
  const foreground = select("Foreground", choices(media, sources, { none: "Nothing in front" }));
  const name = el("input", { type: "text", placeholder: "Virtual set", "aria-label": "Scene name" });
  const make = el("button.btn.primary", { text: "Create the set" });
  const body = el("div.col", {}, [
    el("p.sm.dim", { text: "The presenter is keyed and stands in front of the background. The key colour is found from the camera; fine tune it in the composer." }),
    field("Background", background, "A picture or a looping clip. Upload it in the Media tab first."),
    field("Presenter", presenter, "The camera in front of the green or blue screen."),
    field("Foreground", foreground, "A transparent PNG drawn in front of the presenter, such as a desk."),
    field("Name", name),
  ]);
  const dialog = modal({ title: "Virtual set", body, footer: [el("span.grow"), el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), make] });
  make.onclick = async () => {
    make.disabled = true;
    try {
      const answer = await client.call("scene.virtual_set", request({
        background: background.value, presenter: presenter.value, foreground: foreground.value, name: name.value,
      }));
      dialog.close();
      const how = answer.key_from === "guessed" ? `keyed on ${answer.key}, found in the camera` : answer.key_from === "given" ? `keyed on ${answer.key}` : "the key finds its colour on air";
      toast({ text: `Made "${answer.scene.name}", ${how}.` });
      if (opts.scenes && opts.scenes.refresh) await opts.scenes.refresh();
      if (opts.onMade) opts.onMade(answer);
    } catch (e) {
      make.disabled = false;
      errorToast(e, "Virtual set");
    }
  };
  if (!sources.length) toast({ text: "There are no sources yet. Add the camera first, then make the set." });
  return dialog;
}
