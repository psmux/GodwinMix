// Words over the preview picture where there would otherwise be black: a
// source in the previewed scene that has no picture yet gets its name and
// its state on its own box, and a preview with nothing in it says what to do.
//
// Plain elements over the canvas, placed in percentages of the scene's own
// canvas, so they stay put whatever size the pane is drawn at.

import { el } from "../../shell/dom.js";

const STATES = {
  connecting: "connecting, no picture yet",
  stalled: "stalled, no picture",
  failed: "failed, trying again",
};

/** "connecting", "failed"; null for a source that has a picture to show. */
export function sourceTrouble(s, id) {
  const source = (s.sources || []).find((x) => x.id === id);
  if (!source) return "not in this mixer";
  if (source.state && source.state !== "live") return STATES[source.state] || source.state;
  if (source.has_video === false) return "no video";
  return null;
}

function label(box, title, state) {
  return el("div.preview-slot", { style: box }, [el("strong.ellipsis", { text: title }), el("span", { text: state })]);
}

const WHOLE = { left: "0", top: "0", width: "100%", height: "100%" };

/** The boxes of a scene's sources that have no picture, as labels. */
function sceneSlots(s, kit, id) {
  const view = kit && kit.view(id);
  if (!view) return [];
  const W = (view.canvas && view.canvas.width) || 1920;
  const H = (view.canvas && view.canvas.height) || 1080;
  const out = [];
  for (const box of view.geometry || []) {
    if (!box.source) continue;
    const record = kit.mirror && kit.mirror.record(box.item);
    if (record && record.visible === false) continue;
    const trouble = sourceTrouble(s, box.source);
    if (!trouble) continue;
    const known = (s.sources || []).find((x) => x.id === box.source);
    out.push(label({
      left: `${(box.x / W) * 100}%`, top: `${(box.y / H) * 100}%`,
      width: `${(box.width / W) * 100}%`, height: `${(box.height / H) * 100}%`,
    }, (known && known.name) || box.source, trouble));
  }
  return out;
}

/** Fill the overlay for what Preview holds (studio-next.js's answer). */
export function paintSlots(panel, s, kit, next) {
  const layer = panel.previewSlots;
  if (!layer) return;
  let slots = [];
  if (!next) {
    slots = [label(WHOLE, "Nothing to preview yet", "Add a scene or a source, or click one to put it here.")];
  } else if (next.kind === "source") {
    const trouble = sourceTrouble(s, next.id);
    if (trouble) slots = [label(WHOLE, next.name, trouble)];
  } else {
    slots = sceneSlots(s, kit, next.id);
    const view = kit && kit.view(next.id);
    const empty = view && !(view.geometry || []).length;
    if (empty) slots = [label(WHOLE, next.name, "this scene is empty")];
  }
  if (next && next.why !== "armed" && !(s.multiview && s.multiview.enabled)) {
    slots.push(label(WHOLE, next.name, "Multiview is off, so there is no picture to suggest from"));
  }
  layer.replaceChildren(...slots);
}
