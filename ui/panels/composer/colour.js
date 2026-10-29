// The controls of a colour correction on one item: brightness, contrast,
// gamma, saturation and hue.
//
// Every change is `scene.item.filter.set`, applied to the running filter in
// place, so a slider dragged on air changes the picture on air. The ranges
// are OBS's, so a correction brought across from OBS sits where it was.

import { el, on } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { withParam } from "./key.js";

/** param, label, range and step. Every default is 0, which changes nothing. */
export const SLIDERS = [
  ["brightness", "Brightness", -1, 1, 0.01],
  ["contrast", "Contrast", -4, 4, 0.05],
  ["gamma", "Gamma", -3, 3, 0.05],
  ["saturation", "Saturation", -1, 5, 0.05],
  ["hue_shift", "Hue", -180, 180, 1],
];

/**
 * @param {{filter: object, ref: string|number, set: (ref, params) => Promise}} o
 */
export function colourEditor(o) {
  let params = Object.assign({}, o.filter.params || {});
  let timer = null;
  const send = () => {
    clearTimeout(timer);
    timer = setTimeout(() => o.set(o.ref, params).catch((e) => errorToast(e, "Colour")), 100);
  };
  const sliders = SLIDERS.map(([key, text, min, max, step]) => {
    const value = typeof params[key] === "number" ? params[key] : 0;
    const input = el("input", { type: "range", min: String(min), max: String(max), step: String(step), value: String(value), "aria-label": text });
    const shown = el("span.sm.faint", { text: String(value) });
    on(input, "input", () => {
      shown.textContent = input.value;
      params = withParam(params, key, Number(input.value));
      send();
    });
    return el("label.field", {}, [el("span.lbl", { text }), input, shown]);
  });
  const reset = el("button.btn.sm", { text: "Reset", onclick: () => {
    params = Object.fromEntries(SLIDERS.map(([k]) => [k, 0]));
    for (const s of sliders) {
      s.querySelector("input").value = "0";
      s.querySelector("span.faint").textContent = "0";
    }
    o.set(o.ref, params).catch((e) => errorToast(e, "Colour"));
  } });
  return el("div.col.key-editor", {}, [...sliders, el("div.row", {}, [reset])]);
}
