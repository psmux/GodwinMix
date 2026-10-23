// What is connected to what on a station, with no positions and no DOM:
// nodes in four columns (what comes in, the shows, what each does to the
// picture, what goes out) and the links between them with their rates.
// Rebuilt from each one second snapshot; the same snapshot always makes
// the same graph, so the layout never jumps.
//
// A node is {id, col, kind, tone, title, lines, rate, ...} and a link
// {id, a, b, kbps, tone, err}. `tone` is the colour word every part of the
// map shares: ok, busy, warn, bad or off.

import { addDirect, addInputs, addGpu } from "./graph-direct.js";
import { addMixed } from "./graph-mixed.js";

export { toneOf } from "./tone.js";

export class Graph {
  constructor() {
    this.nodes = new Map();
    this.links = new Map();
  }

  node(n) {
    const had = this.nodes.get(n.id);
    if (had) return Object.assign(had, n);
    const made = { lines: [], rate: 0, ...n };
    this.nodes.set(n.id, made);
    return made;
  }

  link(id, a, b, kbps, tone, extra = {}) {
    this.links.set(id, { id, a, b, kbps: kbps || 0, tone, err: 0, ...extra });
  }

  /** Each node's links in and out, for following a path. */
  index() {
    const into = new Map(), from = new Map();
    for (const l of this.links.values()) {
      if (!into.has(l.b)) into.set(l.b, []);
      if (!from.has(l.a)) from.set(l.a, []);
      into.get(l.b).push(l);
      from.get(l.a).push(l);
    }
    this.into = into;
    this.from = from;
  }
}

/**
 * `d` is {shows, stats, series, mixed, gpu, current, missing, page}.
 * `mixed` holds each mixed show's sources, outputs and programme as
 * mixed.js reads them; `page` is this page's own show for a core with no
 * show.list.
 */
export function build(d) {
  const g = new Graph();
  const shows = d.missing || !d.shows.length ? (d.page ? [d.page] : []) : d.shows;
  for (const show of shows) {
    if (show.compositing === false) addDirect(g, show, d.stats.get(show.id), d.series && d.series.get(show.id));
    else addMixed(g, show, d.mixed.get(show.id) || null);
  }
  addInputs(g, shows, d.stats, d.series);
  addGpu(g, d.gpu);
  g.index();
  return g;
}
