// Turning replay on: which angles, and how far back each keeps. Shown while
// replay is off, and as a Change button while it is on.

import { el } from "../../shell/dom.js";
import { call } from "./panel.js";

export function setup(changed) {
  const seconds = el("input.num.replay-seconds", { type: "number", min: 1, max: 1800, value: 60, "aria-label": "Seconds kept" });
  const boxes = el("div.row.wrap.replay-angle-picks");
  const start = el("button.btn.primary", { text: "Start replay", onclick: () => go() });
  const form = el("div.col", {}, [
    el("div.row", {}, [el("strong.grow", { text: "Replay is off" })]),
    el("div.row.wrap", {}, [el("span.sm.dim", { text: "Keep the last" }), seconds, el("span.sm.dim", { text: "seconds of" })]),
    boxes,
    el("div.row", {}, [start]),
  ]);
  const root = el("div.replay-setup", {}, [form]);
  let shown = "";

  function go() {
    const angles = [...boxes.querySelectorAll("input:checked")].map((i) => i.value);
    call("replay.start", { seconds: Number(seconds.value) || 60, angles }).then(changed, () => {});
  }

  function paint(status, sources) {
    root.hidden = status.on;
    if (status.on) return;
    const ids = ["programme", ...sources.filter((s) => s.id !== "replay" && !String(s.uri || "").startsWith("replay:")).map((s) => s.id)];
    const key = ids.join(",");
    if (key === shown) return;
    shown = key;
    boxes.replaceChildren(...ids.map((id) => {
      const name = id === "programme" ? "Programme" : (sources.find((s) => s.id === id) || {}).name || id;
      return el("label.row.sm", {}, [el("input", { type: "checkbox", value: id, checked: true }), name]);
    }));
  }

  return { root, paint };
}
