// Exposure and focus, out of the way in a drawer: most services never open
// it. Brightness is a slider; focus is automatic unless switched off, then
// nearer and farther move it while held.

import { el } from "../../shell/dom.js";

function sw(label, onchange) {
  const input = el("input", { type: "checkbox", role: "switch", onchange: () => onchange(input.checked) });
  return { input, root: el("label.cam-switch", {}, [input, el("span.cam-switch-track", { "aria-hidden": "true" }), el("span", { text: label })]) };
}

function hold(text, fn) {
  const b = el("button.btn.cam-hold", { type: "button", text });
  let timer = null;
  const off = () => (clearInterval(timer), (timer = null));
  b.addEventListener("pointerdown", () => {
    fn();
    timer = timer || setInterval(fn, 120);
  });
  for (const t of ["pointerup", "pointerleave", "pointercancel"]) b.addEventListener(t, off);
  return b;
}

export function drawer(on) {
  const bright = el("input.cam-range", { type: "range", min: "-1", max: "1", step: "0.05", value: "0", "aria-label": "Brightness" });
  bright.addEventListener("input", () => on.exposure({ brightness: Number(bright.value) }));
  const autoExp = sw("Automatic exposure", (v) => on.exposure({ mode: v ? "auto" : "manual" }));
  const autoFocus = sw("Automatic focus", (v) => on.focus({ auto: v }));
  const near = hold("Nearer", () => on.focus({ speed: -1 }));
  const far = hold("Farther", () => on.focus({ speed: 1 }));
  const root = el("details.cam-drawer", {}, [
    el("summary", { text: "Exposure and focus" }),
    el("div.cam-drawer-body", {}, [
      autoExp.root,
      el("label.cam-field", {}, [el("span", { text: "Brightness" }), bright]),
      autoFocus.root,
      el("div.row", {}, [near, far]),
    ]),
  ]);
  return {
    root,
    show(cam) {
      const e = (cam && cam.exposure) || {};
      const f = (cam && cam.focus) || {};
      autoExp.input.checked = e.mode !== "manual";
      autoFocus.input.checked = f.auto !== false;
      near.disabled = far.disabled = f.auto !== false;
      if (document.activeElement !== bright) bright.value = String(e.brightness || 0);
    },
  };
}

export { sw as switchControl };
