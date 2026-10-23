// The hover card: what is under the pointer, its state, and its numbers,
// with the technical detail in a smaller part at the foot. One element,
// moved and refilled; it never takes the pointer, so the map under it
// keeps answering.

import { el } from "../../shell/dom.js";
import { gpuRows, inputRows, linkRows, outputRows, procRows, programRows, showRows, sourceRows } from "./tip-parts.js";
import { stateWord } from "./words.js";

const ROWS = { input: inputRows, source: sourceRows, show: showRows, proc: procRows, program: programRows, gpu: gpuRows, output: outputRows };
const KIND = { input: "Input", source: "Source", show: "Show", proc: "Conversion", program: "Program", gpu: "GPU", output: "Output" };
const TONE_WORD = { ok: "Live", busy: "Starting", warn: "Needs a look", bad: "Fault", off: "Off" };

const kv = (rows) => el("dl.fm-kv", {}, rows.flatMap(([k, v]) => [el("dt", { text: k }), el("dd", { text: String(v) })]));

export function tip(root) {
  const node = el("div.fm-tip", { hidden: true, role: "tooltip" });
  root.append(node);
  let shown = "";
  return {
    node,
    /** `hit` is {kind: "node"|"link", id}, at screen point x, y; `ctx` what the rows read. */
    show(hit, x, y, ctx) {
      const g = ctx.graph;
      const n = hit.kind === "node" ? g.nodes.get(hit.id) : null;
      const l = hit.kind === "link" ? g.links.get(hit.id) : null;
      if (!n && !l) return this.hide();
      const rows = n ? ROWS[n.kind](n, ctx) : linkRows(l, ctx);
      const tone = (n || l).tone;
      const head = n
        ? [el("span.fm-tkind", { text: KIND[n.kind] }), el("strong", { text: n.num ? `${n.num} ${n.title}` : n.title }), el(`span.fm-tstate.t-${tone}`, { text: n.kind === "source" && n.tally === "program" ? "On air" : n.state ? stateWord(n.state === "running" ? "live" : n.state) : TONE_WORD[tone] })]
        : [el("span.fm-tkind", { text: "Link" }), el("strong", { text: `${(g.nodes.get(l.a) || {}).title} to ${(g.nodes.get(l.b) || {}).title}` })];
      node.replaceChildren(
        el("div.fm-thead", {}, head),
        kv(rows.top),
        rows.tech.length ? el("details.fm-tech", { open: true }, [el("summary", { text: "Technical" }), kv(rows.tech)]) : null,
      );
      node.hidden = false;
      shown = hit.id;
      this.place(x, y);
    },
    place(x, y) {
      const w = root.clientWidth, h = root.clientHeight;
      const r = node.getBoundingClientRect();
      const left = x + 18 + r.width > w ? Math.max(6, x - r.width - 14) : x + 18;
      const top = Math.max(6, Math.min(h - r.height - 6, y - 12));
      node.style.transform = `translate(${Math.round(left)}px, ${Math.round(top)}px)`;
    },
    hide() {
      node.hidden = true;
      shown = "";
    },
    get id() { return shown; },
  };
}
