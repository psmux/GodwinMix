// The controls of a chroma key on one item: its colour, how much counts as
// the screen, the soft edge, the spill, the feather and the garbage matte.
//
// Every change is `scene.item.filter.set`, which the core applies to the
// running key in place: a slider dragged on air moves the key on air with no
// rebuild. The colour can be typed, found from the camera, or picked by a
// click on a still of the camera, which is `source.key_color` either way.

import { el, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";

/** The sliders: param, label, the default the core uses, and the range. */
export const SLIDERS = [
  ["similarity", "Similarity", 0.4, 0, 1, "How far from the key colour still goes fully clear"],
  ["smoothness", "Edge softness", 0.1, 0, 1, "How wide the band between clear and solid is"],
  ["spill", "Spill", 0.6, 0, 1, "How much of the screen's colour is taken off the presenter's edges"],
  ["feather", "Feather", 1, 0, 20, "Pixels the edge of the matte is softened over, inwards"],
  ["matte_left", "Matte left", 0, 0, 0.9, "Cut off this much of the picture on the left"],
  ["matte_right", "Matte right", 0, 0, 0.9, "And on the right"],
  ["matte_top", "Matte top", 0, 0, 0.9, "And at the top"],
  ["matte_bottom", "Matte bottom", 0, 0, 0.9, "And at the bottom"],
];

/** The params with one value changed, leaving the rest as they are. */
export function withParam(params, key, value) {
  return Object.assign({}, params || {}, { [key]: value });
}

/** Where a click on a still lands, as 0 to 1 across and down. */
export function pointOf(event, rect) {
  const clamp = (n) => Math.min(1, Math.max(0, n));
  return { x: clamp((event.clientX - rect.left) / rect.width), y: clamp((event.clientY - rect.top) / rect.height) };
}

/**
 * @param {{client, filter: object, ref: string|number, source: string,
 *          set: (ref, params) => Promise}} o
 */
export function keyEditor(o) {
  let params = Object.assign({}, o.filter.params || {});
  let timer = null;
  const send = (now) => {
    clearTimeout(timer);
    // No redraw of the inspector after a change: it would take the slider
    // out from under a drag. The core applies the change; this keeps its own.
    const go = () => o.set(o.ref, params).catch((e) => errorToast(e, "Key"));
    if (now) go();
    else timer = setTimeout(go, 120);
  };
  const colour = el("input", { type: "color", "aria-label": "Key colour", value: hex(params.color) || "#00ff00" });
  const label = el("span.sm.faint", { text: hex(params.color) ? params.color : "found on air" });
  const setColour = (c) => {
    params = withParam(params, "color", c);
    colour.value = c;
    label.textContent = c;
    send(true);
  };
  on(colour, "input", () => setColour(colour.value));
  const still = el("img.key-still", { alt: "A still of the camera: click the screen to key on it", hidden: true, style: { width: "100%", cursor: "crosshair" } });
  const find = el("button.btn.sm", { text: "Find", title: "Find the screen colour in the camera's picture", onclick: () => ask({}) });
  const pick = el("button.btn.sm", { text: "Pick", title: "Show a still of the camera and click the screen in it", onclick: () => showStill() });
  async function ask(point) {
    try {
      const answer = await o.client.call("source.key_color", Object.assign({ id: o.source }, point));
      setColour(answer.color);
    } catch (e) {
      errorToast(e, "Key colour");
    }
  }
  async function showStill() {
    try {
      const res = await fetch(o.client.snapshotUrl(o.source, 480));
      if (!res.ok) throw new Error("no still of the camera yet; try again in a second");
      if (still.src) URL.revokeObjectURL(still.src);
      still.src = URL.createObjectURL(await res.blob());
      still.hidden = false;
    } catch (e) {
      errorToast(e, "Pick");
    }
  }
  on(still, "click", (e) => ask(pointOf(e, still.getBoundingClientRect())));
  const rows = SLIDERS.map(([key, text, dflt, min, max, tip]) => slider(key, text, dflt, min, max, tip));
  function slider(key, text, dflt, min, max, tip) {
    const value = typeof params[key] === "number" ? params[key] : dflt;
    const input = el("input", { type: "range", min: String(min), max: String(max), step: max > 1 ? "1" : "0.01", value: String(value), "aria-label": text, title: tip });
    const shown = el("span.sm.faint", { text: String(value) });
    on(input, "input", () => {
      const v = Number(input.value);
      shown.textContent = String(v);
      params = withParam(params, key, v);
      send(false);
    });
    return el("label.field", {}, [el("span.lbl", { text }), input, shown]);
  }
  return el("div.col.key-editor", {}, [el("div.row", {}, [el("span.lbl", { text: "Key colour" }), colour, label, find, pick]), still, ...rows]);
}

function hex(value) {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : null;
}
