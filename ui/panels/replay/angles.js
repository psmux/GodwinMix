// The angles: how many seconds and megabytes each holds, what encodes it,
// and which one a play starts on. Picking one while a replay plays changes
// the replay's angle from the frame it is on.

import { el, fmtBytes } from "../../shell/dom.js";
import { quiet, picked } from "./panel.js";

export function angles(pick) {
  const list = el("div.col.replay-angles", { role: "radiogroup", "aria-label": "Replay angle" });
  const root = el("div.col", {}, [el("strong.sm", { text: "Angles" }), list]);
  const rows = new Map();
  let shown = "";

  function choose(id) {
    pick(id);
    // Only does something while a replay plays; otherwise the next play uses it.
    quiet("replay.set", { angle: id });
  }

  function row(a) {
    const fill = el("span");
    const what = el("span.sm.dim.num");
    const radio = el("input", { type: "radio", name: "replay-angle", value: a.id, onchange: () => choose(a.id) });
    const node = el("label.row.replay-angle", { "data-angle": a.id }, [
      radio,
      el("span.replay-angle-name", { text: a.id === "programme" ? "Programme" : a.id }),
      el("span.grow.replay-depth", {}, [fill]),
      what,
    ]);
    return { node, fill, what, radio };
  }

  function paint(status, chosen) {
    root.hidden = !status.on;
    if (!status.on) return;
    const current = chosen || (status.angles[0] && status.angles[0].id);
    if (!picked.angle && current) picked.angle = current;
    const key = status.angles.map((a) => a.id).join(",");
    if (key !== shown) {
      shown = key;
      rows.clear();
      for (const a of status.angles) rows.set(a.id, row(a));
      list.replaceChildren(...[...rows.values()].map((r) => r.node));
    }
    const depth = status.config.seconds || 60;
    for (const a of status.angles) {
      const r = rows.get(a.id);
      const text = a.waiting || `${a.held_secs.toFixed(1)} s, ${fmtBytes(a.bytes)}, ${a.encoder}${a.dropped ? `, ${a.dropped} dropped` : ""}`;
      r.what.textContent = text;
      r.fill.style.width = `${Math.min(100, Math.round((a.held_secs / depth) * 100))}%`;
      r.radio.checked = a.id === current;
      r.node.classList.toggle("on", a.id === current);
    }
  }

  return { root, paint };
}
