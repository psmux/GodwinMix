// Studio mode: the preview beside the programme, and the Take between them.
//
// Loaded the first time Studio mode is switched on, so a page that never uses
// it never downloads it. The panel keeps the programme monitor; this file owns
// the preview pane, the take bar and the keys that send preview to programme.
//
// The take bar is laid out the way a vision mixer's is: Take and Cut first,
// large, in the same place at every width; then one control that says which
// transition Take uses and opens the list of all of them, at most three quick
// picks, and one Effects button. Nothing in it grows with the fx library.
//
// What Preview holds is studio-next.js's answer: the armed scene, a source
// armed on this page, or, with nothing armed, the scene most likely to be
// taken next, so Preview is never just black. Take sends whichever it is.

import { el, on } from "../../shell/dom.js";
import { settings } from "../../shell/settings.js";
import { toast, errorToast } from "../../shell/toast.js";
import { register } from "../../shell/commands.js";
import { acquireScenes } from "../../shell/scene-session.js";
import { releasePreview, retunePreview } from "./studio-picture.js";
import { render } from "./studio-armed.js";
import { picker } from "./transition-picker.js";
import { effects } from "./fx-gallery.js";

export { retunePreview, releasePreview, render };

function previewPane(panel) {
  panel.previewCanvas = el("canvas", { width: 320, height: 180 });
  panel.previewName = el("span.ellipsis");
  panel.previewWhy = el("span.preview-why", { hidden: true });
  panel.previewNote = el("div.preview-note", { role: "status", hidden: true });
  panel.previewSlots = el("div.preview-slots", { "aria-live": "polite" });
  panel.previewWrap = el("div.monitor-pane.preview-pane", {
    title: "Double click to take this to programme",
    ondblclick: () => take(panel, duration(panel)),
  }, [
    el("div.monitor-label", {}, [el("strong", { text: "PREVIEW" }), panel.previewName, panel.previewWhy]),
    panel.previewNote,
    el("div.preview-frame", {}, [panel.previewCanvas, panel.previewSlots]),
  ]);
}

function takeBar(panel) {
  panel.takeBtn = el("button.btn.primary.take-button", {
    type: "button", text: "Take",
    title: "Send preview to programme with the transition below. Space does the same.",
    onclick: () => take(panel, duration(panel)),
  });
  panel.cutBtn = el("button.btn.cut-button", {
    type: "button", text: "Cut",
    title: "Send preview to programme at once, with no transition",
    onclick: () => take(panel, 0),
  });
  const armedScene = () => panel.armedScene || (panel.client && panel.client.state.preview) || null;
  panel.picker = picker({ client: panel.client || null, armedScene });
  panel.effects = panel.client ? effects(panel.client) : null;
  panel.takeBar = el("div.take-bar", { role: "group", "aria-label": "Take" }, [
    el("div.take-main", {}, [panel.takeBtn, panel.cutBtn]),
    el("div.take-more", {}, [panel.picker.el, panel.picker.picks, panel.effects ? panel.effects.button : null]),
    // Fixed to the window when open, so the bar never has to make room.
    panel.picker.sheet, panel.effects ? panel.effects.sheet : null,
  ]);
}

/** The preview pane and the take bar, put into the panel's row once. */
export function build(panel) {
  if (panel.takeBar) return;
  document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./studio.css", import.meta.url).href }));
  previewPane(panel);
  takeBar(panel);
  // The collection's own transitions and the plugins', added to the list.
  if (panel.client) panel.picker.load(panel.client);
  // The scene list, for a suggestion when nothing is armed. Shared with the
  // Scenes panel when that is on the page, so it is read once either way.
  if (panel.client && panel.client.on) {
    panel.sceneSession = acquireScenes(panel.client);
    const offScenes = panel.sceneSession.scenes.onChange(() => panel.render(panel.client.state));
    panel.offs.push(offScenes, () => panel.sceneSession.release());
  }

  const programme = panel.row.querySelector(".program");
  panel.row.insertBefore(panel.previewWrap, programme);
  panel.row.insertBefore(panel.takeBar, programme);
  if (panel.effects) panel.offs.push(panel.effects.off);
  panel.offs.push(
    // A source tile armed in the Sources panel says so here at once, rather
    // than on the next state the core sends.
    on(document, "gmx-armed", () => panel.render(panel.client.state)),
    register({
      id: "program.take-armed",
      title: "Take the preview to programme",
      group: "Programme",
      key: "Space",
      enabled: () => settings().producer,
      run: () => take(panel, duration(panel)),
    }),
    register({
      id: "program.cut-armed",
      title: "Cut the preview to programme",
      group: "Programme",
      enabled: () => settings().producer,
      run: () => take(panel, 0),
    }),
  );
}

function duration(panel) {
  return panel.picker ? panel.picker.ms() : 500;
}

/** Shown in Studio mode and hidden, with its pictures let go, outside it. */
export function show(panel, producer) {
  if (!panel.takeBar) return;
  panel.previewWrap.hidden = panel.takeBar.hidden = !producer;
  panel.classList.toggle("studio", producer);
  if (!producer) {
    panel.picker.close();
    if (panel.effects) panel.effects.close();
    releasePreview(panel);
  }
}

/** Preview to programme: a cut with no length, the chosen transition with one. */
export async function take(panel, durationMs) {
  const next = panel.next;
  if (!next) {
    toast({ text: "Nothing is in preview. Add a scene or a source, then click it." });
    return;
  }
  const request = next.kind === "scene" ? { scene: next.why === "armed" ? next.name : next.id } : { source: next.id };
  // Cut says so, so a default transition set with fx.assign does not turn
  // the Cut button into a Take.
  request.transition = durationMs ? (panel.picker ? panel.picker.request(durationMs) : { type: "fade", duration_ms: durationMs }) : "cut";
  try {
    await panel.client.call("program.take", request);
    if (durationMs && panel.picker) panel.picker.used();
  } catch (e) {
    errorToast(e, "Take");
  }
}
