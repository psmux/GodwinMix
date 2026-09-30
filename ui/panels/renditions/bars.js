// A capacity bar: what is in use, piece by piece, what could still be
// admitted, and what the governor keeps free for what is already on air.
// Plain elements and CSS widths, no chart library. Every piece has a title
// for a hover, and the legend under the bar names each one in words.

import { el } from "../../shell/dom.js";

/**
 * @param {number} total  the whole capacity, in the same unit as the rest
 * @param {{label: string, value: number, title?: string}[]} used
 * @param {number} room   what could still be admitted
 * @param {(n: number) => string} say  a value in words
 */
export function capacityBar(total, used, room, say) {
  const sum = used.reduce((n, u) => n + u.value, 0);
  const kept = Math.max(0, total - sum - room);
  const pct = (n) => `${Math.max(0, Math.min(100, (100 * n) / (total || 1)))}%`;
  const pieces = used.filter((u) => u.value > 0).map((u) => el("span.rnd-seg", { style: { width: pct(u.value) }, title: `${u.label}: ${say(u.value)}` }));
  if (room > 0) pieces.push(el("span.rnd-seg.room", { style: { width: pct(room) }, title: `Room for more: ${say(room)}` }));
  if (kept > 0) pieces.push(el("span.rnd-seg.kept", { style: { width: pct(kept) }, title: `Kept free for what is on air and the page: ${say(kept)}` }));
  const bar = el("div.rnd-bar", { role: "img", "aria-label": `${say(sum)} in use, room for ${say(room)} more` }, pieces);
  const legend = el("ul.rnd-legend", {}, [
    ...used.filter((u) => u.value > 0).map((u) => row("in", u.label, say(u.value), u.title)),
    row("room", "Room for more", say(room)),
    kept > 0 ? row("kept", "Kept free", say(kept), "What is on air and the page need, which nothing new may take") : null,
  ]);
  return el("div.rnd-barwrap", {}, [bar, legend]);
}

function row(kind, label, value, title) {
  return el("li", { title: title || "" }, [el(`span.rnd-key.${kind}`), el("span.rnd-lname", { text: label }), el("span.rnd-lval", { text: value })]);
}

/** Encoder sessions as pips: filled for each one held, hollow for each free. */
export function sessionPips(used, max) {
  const n = Math.min(Math.max(max || 0, used || 0), 32);
  const pips = Array.from({ length: n }, (_, i) => el("span.rnd-pip" + (i < used ? ".on" : "")));
  return el("div.rnd-pips", { role: "img", "aria-label": `${used} of ${max} encoder sessions in use` }, [
    el("span.rnd-pipset", {}, pips),
    el("span.rnd-lval", { text: max ? `${used} of ${max} sessions` : `${used} sessions, no limit` }),
  ]);
}
