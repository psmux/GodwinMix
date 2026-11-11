// A row of input buttons, the way a switcher's panel has them: the programme
// row lights red on what is on air, the preview row green on what is next.

import { el } from "../../shell/dom.js";

/** Cameras and plain inputs; black, bars and the media players only in Pro. */
export const plainInputs = (atem) => (atem.inputs || []).filter((i) => i.id > 0 && i.id < 1000);

export function bus(label, kind, onPick) {
  const row = el("div.cam-bus-keys", { role: "radiogroup", "aria-label": label });
  const root = el("div.cam-bus." + kind, {}, [el("span.cam-bus-label", { text: label }), row]);
  let key = "";
  return {
    root,
    show(inputs, current) {
      const k = inputs.map((i) => i.id + i.short).join();
      if (k !== key) {
        key = k;
        row.replaceChildren(...inputs.map((i) =>
          el("button.cam-key", { type: "button", role: "radio", title: i.name, "data-input": String(i.id), text: i.short || i.name, onclick: () => onPick(i.id) })));
      }
      for (const b of row.children) {
        const on = Number(b.dataset.input) === current;
        b.classList.toggle("on", on);
        b.setAttribute("aria-checked", String(on));
      }
    },
  };
}
