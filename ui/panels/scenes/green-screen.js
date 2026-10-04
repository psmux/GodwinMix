// New scene, started from the presenter layout: a presenter put in front of
// a new background in one step, with a green screen, a blue one, or no screen
// at all, when the person is cut out by a model.
//
// It is a scene like any other, made by `scene.create_from` with the
// `virtual-set` layout, the same call an agent makes. That call turns library
// pictures into sources and guesses the key colour from the camera. The
// dialog asks three things: the picture behind (from the media library or a
// source), the camera, and an optional picture in front such as a desk.
// Fetched on open, never before.

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

/** The request the dialog sends, from what is picked: the sources in the
 * layout's slot order, background, presenter, then what stands in front. */
export function request(picked) {
  const sources = [picked.background, picked.presenter];
  if (picked.foreground) sources.push(picked.foreground);
  const name = picked.name && picked.name.trim() ? picked.name.trim() : "Presenter";
  const body = { sources, layout: "virtual-set", name };
  if (picked.screen && picked.screen !== "green") body.settings = { screen: picked.screen };
  return body;
}

/** What can be behind the presenter. */
export const SCREENS = [
  ["green", "A green screen"],
  ["blue", "A blue screen"],
  ["none", "No screen: cut the person out"],
];

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
export async function openGreenScreen(opts) {
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
  const screen = select("Behind the presenter", SCREENS.map(([value, label]) => ({ value, label })));
  const name = el("input", { type: "text", placeholder: "Presenter", "aria-label": "Scene name" });
  const make = el("button.btn.primary", { text: "Make the scene" });
  const body = el("div.col", {}, [
    el("p.sm.dim", { text: "The presenter stands in front of the background. With a screen behind them the key colour is found from the camera; with none, a model cuts them out. Fine tune either in the composer." }),
    field("Behind the presenter", screen, "No screen works anywhere; a screen gives the cleanest edge."),
    field("Background", background, "A picture or a looping clip. Upload it in the Media tab first."),
    field("Presenter", presenter, "The camera in front of the green or blue screen."),
    field("Foreground", foreground, "A transparent PNG drawn in front of the presenter, such as a desk."),
    field("Name", name),
  ]);
  const dialog = modal({ title: "New scene: presenter in front of a new background", body, footer: [el("span.grow"), el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), make] });
  make.onclick = async () => {
    make.disabled = true;
    try {
      const answer = await client.call("scene.create_from", request({
        background: background.value, presenter: presenter.value, foreground: foreground.value, name: name.value, screen: screen.value,
      }));
      dialog.close();
      const how = screen.value === "none" ? "the presenter cut out by a model" : answer.key_from === "guessed" ? `keyed on ${answer.key}, found in the camera` : answer.key_from === "given" ? `keyed on ${answer.key}` : "the key finds its colour on air";
      toast({ text: `Made "${answer.name}", ${how}.` });
      if (opts.scenes && opts.scenes.refresh) await opts.scenes.refresh();
      if (opts.onMade) opts.onMade(answer);
    } catch (e) {
      make.disabled = false;
      errorToast(e, "New scene");
    }
  };
  if (!sources.length) toast({ text: "There are no sources yet. Add the camera first, then make the scene." });
  return dialog;
}
