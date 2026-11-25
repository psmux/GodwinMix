// How it follows: the ptz plugin's track_tune numbers in plain words, folded
// under the framing in camera properties and closed until somebody opens it.
// Each slider sends at most ten times a second while it moves and once more
// when it is let go, as the audio chain's do. A refusal says the range, and
// that sentence is what is shown.

import { el } from "../../shell/dom.js";

const KNOBS = [
  // key, words, min, max, step, flip (right is the lower number), shown as
  ["max_speed", "Speed", 0.05, 1, 0.05, false, (v) => `${Math.round(v * 100)}%`],
  ["max_accel", "Smoothness", 0.2, 10, 0.1, true, (v) => (v <= 1.5 ? "very smooth" : v <= 4 ? "smooth" : "quick")],
  ["headroom", "Room above the head", 0, 0.4, 0.01, false, (v) => `${Math.round(v * 100)}%`],
  ["lead_room", "Lead room", 0, 2, 0.1, false, (v) => `${v.toFixed(1)} s`],
  ["dead_zone", "Steadiness", 0, 0.3, 0.01, false, (v) => `${Math.round(v * 100)}%`],
  ["lost_hold_ms", "Hold when lost", 0, 30000, 500, false, (v) => `${(v / 1000).toFixed(1)} s`],
];
const EVERY_MS = 100;

/** `send(changes)` calls track_tune with them and answers its answer. */
export function followTuning(send, presets) {
  const said = el("p.sm.cam-tune-said", { hidden: true });
  const knobs = KNOBS.map(([key, words, min, max, step, flip, show]) => {
    const input = el("input", { type: "range", min: String(min), max: String(max), step: String(step), "aria-label": words });
    const value = el("span.sm.dim.num");
    const read = () => (flip ? min + max - Number(input.value) : Number(input.value));
    let timer = null;
    const go = () => {
      timer = null;
      value.textContent = show(read());
      push({ [key]: key === "lost_hold_ms" ? Math.round(read()) : Math.round(read() * 1000) / 1000 });
    };
    input.addEventListener("input", () => {
      value.textContent = show(read());
      if (!timer) timer = setTimeout(go, EVERY_MS);
    });
    input.addEventListener("change", () => (clearTimeout(timer), go()));
    const root = el("label.cam-tune", {}, [el("span.row", {}, [el("span.grow", { text: words }), value]), input]);
    return { key, input, value, root, set: (v) => document.activeElement !== input && ((input.value = String(flip ? min + max - v : v)), (value.textContent = show(v))) };
  });
  const lost = el("select", { "aria-label": "When lost", onchange: () => push({ fallback: lost.value }) });
  const root = el("details.panel-section.cam-more", {}, [
    el("summary", { text: "How it follows" }),
    el("div.col.pad", {}, [...knobs.map((k) => k.root), el("label.cam-tune", {}, [el("span", { text: "When lost" }), lost]), said]),
  ]);

  async function push(changes) {
    try {
      const r = await send(changes);
      said.hidden = true;
      if (r && r.tuning) show(r.tuning);
    } catch (e) {
      said.textContent = (e && e.message) || String(e);
      said.hidden = false;
    }
  }

  function show(t) {
    for (const k of knobs) if (typeof t[k.key] === "number") k.set(t[k.key]);
    const shots = (presets() || []).map((p) => [`preset:${p.slot}`, `Go to ${p.name}`]);
    const options = [["stay", "Stay where it is"], ["wide", "Go wide"], ...shots];
    if (lost.options.length !== options.length) lost.replaceChildren(...options.map(([v, t]) => el("option", { value: v, text: t })));
    if (document.activeElement !== lost && t.fallback) lost.value = t.fallback;
  }

  // Read when opened, so a closed section costs nothing.
  root.addEventListener("toggle", () => root.open && push({}));
  return { root, show };
}
