// Exposure and focus, folded away at the foot of camera properties: most
// services never open it. Exposure is the camera's own mode and, off automatic, its iris,
// shutter and gain in the steps its manual numbers them. Focus is automatic
// unless switched off; then Nearer and Farther drive it while held, and
// Focus once sharpens on the middle of the picture and holds.

import { el } from "../../shell/dom.js";
import { switchControl as sw } from "./controls.js";

/** A button that does `down` while pressed and `up` when let go. */
function hold(text, down, up) {
  const b = el("button.btn.sm.cam-hold", { type: "button", text });
  let held = false;
  const off = () => held && ((held = false), up());
  b.addEventListener("pointerdown", () => ((held = true), down()));
  for (const t of ["pointerup", "pointerleave", "pointercancel"]) b.addEventListener(t, off);
  return b;
}

const MODES = [["auto", "Auto"], ["manual", "Manual"], ["shutter", "Shutter"], ["iris", "Iris"]];

function step(label, key, on) {
  const input = el("input.num", { type: "number", min: "0", step: "1", "aria-label": label });
  input.addEventListener("change", () => input.value !== "" && on.exposure({ [key]: Math.round(Number(input.value)) }));
  return { key, input, root: el("label.cam-field", {}, [el("span", { text: label }), input]) };
}

export function exposureFocus(on) {
  const mode = el("div.cam-seg", { role: "radiogroup", "aria-label": "Exposure mode" },
    MODES.map(([v, t]) => el("button.btn.sm", { type: "button", role: "radio", text: t, "data-v": v, onclick: () => on.exposure({ mode: v }) })));
  const steps = [step("Iris", "iris", on), step("Shutter", "shutter", on), step("Gain", "gain", on)];
  const manual = el("div.cam-row-wrap", {}, steps.map((s) => s.root));
  const autoFocus = sw("Automatic focus", (v) => on.focus(v ? "auto" : "manual"));
  const once = el("button.btn.sm.cam-hold", { type: "button", text: "Focus once", title: "Sharpen on the middle of the picture, then hold", onclick: () => on.focus("one_push") });
  const near = hold("Nearer", () => on.focus("near", 0.5), () => on.focus("stop"));
  const far = hold("Farther", () => on.focus("far", 0.5), () => on.focus("stop"));
  const root = el("details.panel-section.cam-more", {}, [
    el("summary", { text: "Exposure and focus" }),
    el("div.col.pad", {}, [
      el("div.cam-label", { text: "Exposure" }),
      mode,
      manual,
      el("div.cam-label", { text: "Focus" }),
      autoFocus.root,
      el("div.cam-row-wrap", {}, [once, near, far]),
    ]),
  ]);
  return {
    root,
    /** `state` is {exposure: {mode, iris, shutter, gain}, focus: {mode}}, as the camera last answered. */
    show(state) {
      const e = (state && state.exposure) || {};
      const f = (state && state.focus) || {};
      const m = e.mode || "auto";
      for (const b of mode.children) b.setAttribute("aria-checked", String(b.dataset.v === m));
      manual.hidden = m === "auto";
      for (const s of steps) if (document.activeElement !== s.input) s.input.value = e[s.key] ?? "";
      const auto = !f.mode || f.mode === "auto";
      autoFocus.input.checked = auto;
      near.disabled = far.disabled = auto;
    },
  };
}
