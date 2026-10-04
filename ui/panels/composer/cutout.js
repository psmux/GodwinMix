// The controls of a background cutout on one item: which model, where it
// runs, and how the edge sits.
//
// The partner of key.js, for a camera with no green screen behind it. Every
// change is `scene.item.filter.set`, applied to the running cutout in place,
// so a slider dragged on air moves the edge on air.

import { el, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { withParam } from "./key.js";

/** The choices: param, label, options and what each means. */
export const CHOICES = [
  ["quality", "Model", [
    ["auto", "Automatic: fine on a GPU, fast without one"],
    ["fast", "Fast: any computer, softer edge"],
    ["fine", "Fine: hair and hands, wants a GPU"],
  ]],
  ["device", "Runs on", [
    ["auto", "Automatic: the GPU if there is one"],
    ["gpu", "The GPU"],
    ["cpu", "The CPU"],
  ]],
];

/** The sliders: param, label, the default the core uses, and the range. */
export const SLIDERS = [
  ["cutoff", "Edge position", 0.5, 0, 1, "Lower keeps more around the person, higher keeps less"],
  ["softness", "Edge softness", 0.3, 0, 1, "How wide the soft band at the edge is; 0 is a hard edge"],
  ["steady", "Steadiness", 0.5, 0, 0.95, "How much the edge carries from one frame to the next; higher shimmers less and follows fast movement later"],
  ["feather", "Feather", 0, 0, 20, "Pixels the edge is softened over, inwards"],
  ["matte_left", "Matte left", 0, 0, 0.9, "Cut off this much of the picture on the left, whatever the model says"],
  ["matte_right", "Matte right", 0, 0, 0.9, "And on the right"],
  ["matte_top", "Matte top", 0, 0, 0.9, "And at the top"],
  ["matte_bottom", "Matte bottom", 0, 0, 0.9, "And at the bottom"],
];

/**
 * @param {{filter: object, ref: string|number, set: (ref, params) => Promise}} o
 */
export function cutoutEditor(o) {
  let params = Object.assign({}, o.filter.params || {});
  let timer = null;
  const send = (now) => {
    clearTimeout(timer);
    const go = () => o.set(o.ref, params).catch((e) => errorToast(e, "Cutout"));
    if (now) go();
    else timer = setTimeout(go, 120);
  };
  const choices = CHOICES.map(([key, text, options]) => {
    const box = el("select", { "aria-label": text });
    for (const [value, label] of options) box.appendChild(el("option", { value, text: label }));
    box.value = typeof params[key] === "string" ? params[key] : "auto";
    on(box, "change", () => {
      params = withParam(params, key, box.value);
      send(true);
    });
    return el("label.field", {}, [el("span.lbl", { text }), box]);
  });
  const sliders = SLIDERS.map(([key, text, dflt, min, max, tip]) => {
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
  });
  return el("div.col.key-editor", {}, [
    el("p.sm.dim", { text: "The person is cut out by a model, with no screen behind them. Put a picture, a clip or a page below this item and it becomes the background." }),
    ...choices,
    ...sliders,
  ]);
}
