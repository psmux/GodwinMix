// The live flow map on one canvas, after Firmament's network map. The
// layout is made again from each one second snapshot, and the links and
// cards are painted into two cached layers then, or when the view moves or
// the pointer finds something; each animation frame only lays the layers
// down and draws the moving dots between them (frame.js). The view is a
// scale and an offset over the map's own coordinates.

import { layout } from "./layout.js";
import { shape } from "./geom.js";
import { expected } from "./dots.js";
import { paintCards, paintHeads, paintLinks } from "./paint.js";
import { frame } from "./frame.js";

const TOKENS = ["text", "dim", "faint", "panel", "panel2", "raise", "line", "line-soft", "ok", "warn", "bad", "accent", "live", "bg", "font", "num"];
const HEADS = ["Coming in", "Shows", "Processing", "Going out"];

export class FlowMap {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext("2d");
    this.under = document.createElement("canvas");
    this.over = document.createElement("canvas");
    this.v = { s: 1, tx: 16, ty: 0, w: 0, h: 0, dpr: 1 };
    this.nodes = [];
    this.links = [];
    this.byId = new Map();
    this.pulses = [];
    this.look = { lit: null, hover: null, selected: null, dims: () => false };
    this.stats = { frames: 0, work: 0, fps: 0, avgWork: 0, maxWork: 0, dots: 0, t0: 0 };
    this.reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
    this.tSnap = 0;
    this.budget = 1;
    this.colours();
  }

  colours() {
    const cs = getComputedStyle(this.canvas);
    this.col = Object.fromEntries(TOKENS.map((k) => [k.replace("-s", "S"), cs.getPropertyValue(`--${k}`).trim() || "#888"]));
    this.col.link = this.col.faint;
    const family = this.col.font, mono = this.col.num;
    this.font = (w, px, kind) => `${w} ${px}px ${kind === "num" ? mono : family}`;
    this.dirty = true;
  }

  resize(w, h) {
    this.v.dpr = Math.min(2, window.devicePixelRatio || 1);
    Object.assign(this.v, { w, h });
    for (const c of [this.canvas, this.under, this.over]) {
      c.width = Math.round(w * this.v.dpr);
      c.height = Math.round(h * this.v.dpr);
    }
    Object.assign(this.canvas.style, { width: `${w}px`, height: `${h}px` });
    this.dirty = true;
  }

  /** A new snapshot: lay it out, keep each surviving link's dots, ease its rate. */
  set(graph, narrow) {
    const now = performance.now();
    const k = this.ease(now);
    const had = new Map(this.links.map((l) => [l.id, l]));
    const tones = new Map(this.nodes.map((n) => [n.id, n.tone]));
    this.box = layout(graph, narrow);
    this.graph = graph;
    this.nodes = [...graph.nodes.values()];
    this.byId = graph.nodes;
    this.links = [...graph.links.values()].filter((l) => graph.nodes.has(l.a) && graph.nodes.has(l.b)).map((l) => {
      const old = had.get(l.id);
      const geo = shape(l, graph.nodes.get(l.a), graph.nodes.get(l.b));
      const from = old ? old.k0 + (old.kbps - old.k0) * k : l.kbps;
      return Object.assign(l, geo, { k0: from, parts: old ? old.parts : [], acc: old ? old.acc : Math.random() });
    });
    for (const n of this.nodes) {
      const was = tones.get(n.id);
      if (was && was !== n.tone) this.pulse(n);
    }
    this.tSnap = now;
    this.budget = Math.min(1, 3600 / Math.max(1, expected(this.links)));
    this.dirty = true;
  }

  ease(now) {
    return Math.max(0, Math.min(1, (now - this.tSnap) / 900));
  }

  pulse(n) {
    this.pulses.push({ id: n.id, t0: performance.now(), tone: n.tone });
  }

  /** Rebuild the two still layers, when something has changed them. */
  paint() {
    if (!this.dirty || !this.v.w) return;
    this.dirty = false;
    paintLinks(this.under.getContext("2d"), this.links, this.v, this.col, this.look);
    const g = this.over.getContext("2d");
    paintCards(g, this.nodes, this.v, this.col, this.look, this.font);
    if (this.box) paintHeads(g, this.box.cols, this.v, this.col, this.font, HEADS);
  }

  /** Fit the whole map, or its width only when the rows would be too small to read. */
  fit(all = true) {
    if (!this.box) return;
    const pad = 16;
    const sx = (this.v.w - pad * 2) / this.box.width, sy = (this.v.h - pad) / this.box.height;
    const s = Math.max(0.15, Math.min(1.25, all ? Math.min(sx, sy) : Math.min(sx, 1.1)));
    this.v.s = s;
    this.v.tx = (this.v.w - this.box.width * s) / 2;
    this.v.ty = all && sy >= sx ? (this.v.h - this.box.height * s) / 2 : 8;
    this.dirty = true;
  }

  zoomAt(x, y, f) {
    const s = Math.max(0.12, Math.min(3, this.v.s * f));
    this.v.tx = x - ((x - this.v.tx) * s) / this.v.s;
    this.v.ty = y - ((y - this.v.ty) * s) / this.v.s;
    this.v.s = s;
    this.dirty = true;
  }

  pan(dx, dy) {
    this.v.tx += dx;
    this.v.ty += dy;
    this.dirty = true;
  }

  /** A point on screen in the map's units. */
  world(x, y) {
    return [(x - this.v.tx) / this.v.s, (y - this.v.ty) / this.v.s];
  }

  start() {
    if (this.running) return;
    this.running = true;
    this.last = performance.now();
    const loop = (t) => {
      if (!this.running) return;
      if (!this.reduced || this.dirty || this.pulses.length) frame(this, t, this.reduced);
      this.raf = requestAnimationFrame(loop);
    };
    this.raf = requestAnimationFrame(loop);
  }

  stop() {
    this.running = false;
    cancelAnimationFrame(this.raf);
  }
}
