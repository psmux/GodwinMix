// New scene, started from the presenter layout: a presenter put in front of
// a new background in one step, with a green screen, a blue one, or no screen
// at all, when the person is cut out by a model.
//
// It is a scene like any other, made by `scene.create_from` with the
// `virtual-set` layout, the same call an agent makes. That call turns library
// pictures into sources and guesses the key colour from the camera. The
// dialog is three rows of pictures to pick from: the camera, what is behind
// the person now, and the new background (the gallery's backgrounds, library
// files, or one uploaded on the spot). Something in front and the scene's
// name are under Advanced. Fetched on open, never before.

import { el, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { previewUrl } from "../graphics/model.js";
import { tileGroup, step, backgrounds, people } from "./green-screen-tiles.js";

export { backgrounds };

/** What a picker offers: library files first, then sources. */
export function choices(media, sources, opts = {}) {
  const out = [];
  if (opts.none) out.push({ value: "", label: opts.none });
  for (const item of media || []) out.push({ value: item.name, label: `${item.name} (library)` });
  for (const s of sources || []) out.push({ value: s.id, label: s.name && s.name !== s.id ? `${s.name} (${s.id})` : s.id });
  return out;
}

/** The presenter to offer first: a camera, a phone or a feed from another
 * machine before a test pattern, a picture or a page, which are rarely a
 * person. The first source when nothing looks like a camera. */
export function likelyPresenter(sources) {
  const live = /camera|decklink|ndi|whip|guest|rtmp|srt|ipcam|device|capture/i;
  const hit = (sources || []).find((s) => live.test(`${s.type || ""} ${s.uri || ""}`));
  return hit ? hit.id : (sources || [])[0]?.id || "";
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

/** What can be behind the presenter: value, label, and the swatch a tile shows. */
export const SCREENS = [
  ["none", "An ordinary room", "room"],
  ["green", "A green screen", "green"],
  ["blue", "A blue screen", "blue"],
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
  if (!document.querySelector('link[href$="/scenes/green-screen.css"]')) {
    document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./green-screen.css", import.meta.url).href }));
  }
  const listed = async (method, params) => {
    try {
      return ((await client.call(method, params)) || {}).items || [];
    } catch {
      return []; // A core with no library or gallery still has its sources.
    }
  };
  const [media, gallery] = await Promise.all([listed("media.list", {}), listed("gallery.list", { limit: 500 })]);
  const sources = client.state.sources || [];
  const picture = (id) => (client.snapshotUrl ? `${client.snapshotUrl(id, 240)}&force=true` : undefined);
  const presenter = tileGroup("Presenter", people(sources, likelyPresenter(sources)).map((s) => ({ value: s.id, label: s.name || s.id, img: picture(s.id) })), likelyPresenter(sources));
  const screen = tileGroup("Behind the presenter", SCREENS.map(([value, label, swatch]) => ({ value, label, swatch, note: value === "none" ? "Cut out automatically" : "Keyed out" })), "none");
  const options = backgrounds(client, gallery, media);
  const background = tileGroup("Background", options, options[0] ? options[0].value : "");
  const file = el("input", { type: "file", accept: "image/*,video/*", hidden: true });
  const uploadTile = el("button.gs-tile.gs-upload", { type: "button", onclick: () => file.click() }, [el("span.gs-face.gs-none", { text: "+" }), el("span.gs-label", { text: "Upload your own" })]);
  background.insertBefore(uploadTile, background.firstChild);
  on(file, "change", async () => {
    const f = file.files[0];
    if (!f) return;
    uploadTile.disabled = true;
    try {
      const { upload } = await import("../graphics/actions.js");
      const done = await upload(client, f);
      const ids = (done.added || []).map((a) => (typeof a === "string" ? a : a.id));
      const fresh = (await listed("gallery.list", { limit: 500 })).find((i) => ids.includes(i.id) && i.uri);
      const o = fresh ? { value: fresh.uri, label: fresh.name, img: previewUrl(client, fresh, 240) } : null;
      if (o) {
        background.add(o, true);
        background.insertBefore(uploadTile, background.firstChild);
        background.value = o.value;
      }
    } catch (e) {
      errorToast(e, "Upload");
    }
    uploadTile.disabled = false;
  });
  const foreground = select("Foreground", choices(media, sources, { none: "Nothing in front" }));
  const name = el("input", { type: "text", placeholder: "Presenter", "aria-label": "Scene name" });
  const make = el("button.btn.primary", { text: "Make the scene" });
  const body = el("div.col.gs-body", {}, [
    step(1, "Who is presenting?", "The camera pointed at the person."),
    presenter,
    step(2, "What is behind them now?", "Most people have no screen. That is fine."),
    screen,
    step(3, "Pick the new background", "A picture, a clip or a studio page."),
    background,
    file,
    el("details.gs-advanced", {}, [el("summary.sm", { text: "Advanced: something in front, and the scene's name" }), field("Foreground", foreground, "A transparent PNG drawn in front of the presenter, such as a desk."), field("Name", name)]),
  ]);
  const dialog = modal({ title: "Put a presenter in front of a new background", body, footer: [el("span.grow"), el("button.btn", { text: "Cancel", onclick: () => dialog.close() }), make] });
  make.onclick = async () => {
    make.disabled = true;
    try {
      // A studio set is placed, which makes the scene with its own desk and
      // the presenter where the set puts them.
      const set = background.value.startsWith("set:") ? background.value.slice(4) : null;
      const answer = set
        ? await client.call("gallery.place", { id: set, camera: presenter.value, screen: screen.value })
        : await client.call("scene.create_from", request({
          background: background.value, presenter: presenter.value, foreground: foreground.value, name: name.value, screen: screen.value,
        }));
      dialog.close();
      const how = screen.value === "none" ? "the presenter cut out by a model" : answer.key_from === "guessed" ? `keyed on ${answer.key}, found in the camera` : answer.key_from === "given" ? `keyed on ${answer.key}` : "the key finds its colour on air";
      toast({ text: `Made "${answer.scene || answer.name}", ${how}.` });
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
