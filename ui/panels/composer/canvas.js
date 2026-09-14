// The composer's canvas: the picture, the handles over it, and the pointer.
//
// The picture comes from whatever transport is available and is never drawn by
// this file. What this file owns is the overlay: the item outlines, the handles
// the plugin declared, the snap guides, the safe areas and the marquee, and
// turning a press and a drag into `scene.item.set`.
//
// Drag at input rate (11 section 4). A pointer move draws immediately from a
// local prediction and sends the same move with a sequence number; the core's
// answer is the echo, and the drawing snaps to it only once the numbers meet.
// Both times are measured, because 07 Phase 3 asks for numbers: the redraw
// within 16 ms, the echo under 25 ms on the same host.

import { el, on } from "../../shell/dom.js";
import { Prediction } from "../../kits/protocol/predict.js";
import { Timings, now } from "../../kits/protocol/timing.js";
import { View, rectFrom, overlaps, inside, unionOf } from "../../kits/canvas/geometry.js";
import { gizmosFor, handlesFor, hitTest, applyDrag } from "../../kits/canvas/gizmos.js";
import { snapTargets, snapDelta, pointsOf } from "../../kits/canvas/snap.js";
import { paint, grabRadius } from "../../kits/canvas/draw.js";

/** Travel in CSS pixels before a press becomes a drag rather than a click. */
const THRESHOLD = 4;

export class ComposerCanvas {
  /**
   * @param {{scenes: object, onSelect: Function, designerFor: Function,
   *          onEdit?: Function}} opts
   */
  constructor(opts) {
    this.o = opts;
    this.scenes = opts.scenes;
    this.prediction = new Prediction();
    this.timings = new Timings();
    this.selection = new Set();
    this.boxes = new Map();
    this.view = null;
    this.viewport = new View({ width: 1920, height: 1080 }, { width: 1, height: 1 });
    this.safe = true;
    this.rulers = false;
    this.snap = true;

    this.picture = el("div.composer-picture");
    this.overlay = el("canvas.composer-overlay");
    this.el = el("div.composer-canvas", {}, [this.picture, this.overlay]);

    this.offs = [
      on(this.overlay, "pointerdown", (e) => this.down(e)),
      on(this.overlay, "pointermove", (e) => this.move(e)),
      on(this.overlay, "pointerup", (e) => this.up(e)),
      on(this.overlay, "pointercancel", () => this.cancel()),
      on(this.overlay, "dblclick", (e) => e.preventDefault()),
    ];
    this.ro = new ResizeObserver(() => this.resize());
    this.ro.observe(this.el);
  }

  destroy() {
    for (const off of this.offs) off();
    this.ro.disconnect();
  }

  // ----------------------------------------------------------------- state

  /** A fresh view of the draft: the records and the flattened geometry. */
  setView(view) {
    this.view = view;
    if (view && view.canvas) this.viewport.set(view.canvas, this.surface());
    for (const box of (view && view.geometry) || []) {
      // An item this client is still dragging keeps the box it drew: the
      // core's older answer would pull the handle backwards under the hand.
      if (this.prediction.pending.has(box.item)) continue;
      this.boxes.set(box.item, { x: box.x, y: box.y, width: box.width, height: box.height });
    }
    const alive = new Set(((view && view.geometry) || []).map((g) => g.item));
    for (const id of [...this.boxes.keys()]) if (!alive.has(id)) this.boxes.delete(id);
    this.draw();
  }

  setSelection(ids) {
    this.selection = new Set(ids);
    this.draw();
  }

  resize() {
    const box = this.surface();
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    this.overlay.width = Math.max(1, Math.round(box.width * dpr));
    this.overlay.height = Math.max(1, Math.round(box.height * dpr));
    this.overlay.style.width = box.width + "px";
    this.overlay.style.height = box.height + "px";
    this.ctx = this.overlay.getContext("2d");
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (this.view && this.view.canvas) this.viewport.set(this.view.canvas, box);
    this.draw();
  }

  surface() {
    const r = this.el.getBoundingClientRect();
    return { width: r.width || 1, height: r.height || 1 };
  }

  /** The geometry a drawing pass uses: predicted where a drag is in flight. */
  entries() {
    const out = [];
    for (const record of this.items()) {
      const box = this.boxes.get(record.id);
      if (box) out.push({ id: record.id, box, label: record.name || "" });
    }
    return out;
  }

  items() {
    if (!this.view) return [];
    return (this.view.records || []).filter((r) => r.kind === "item");
  }

  record(id) {
    return (this.view && (this.view.records || []).find((r) => r.id === id)) || null;
  }

  // ------------------------------------------------------------------ draw

  draw() {
    if (this.pendingFrame) return;
    this.pendingFrame = requestAnimationFrame(() => {
      this.pendingFrame = null;
      if (!this.ctx) this.resize();
      if (!this.ctx) return;
      this.timings.measure("redraw", () => this.paintNow());
    });
  }

  /** Straight away, no frame in between: what a drag measures. */
  paintNow() {
    paint(this.ctx, {
      view: this.viewport,
      boxes: this.entries(),
      selected: this.selection,
      handles: this.handles(),
      guides: this.guides || [],
      marquee: this.state && this.state.mode === "marquee" ? this.state.rect : null,
      safe: this.safe,
      rulers: this.rulers,
      root: this.el,
    });
  }

  /** The handles for the one selected item, from its plugin's designer block. */
  handles() {
    if (this.selection.size !== 1) {
      const boxes = [...this.selection].map((id) => this.boxes.get(id)).filter(Boolean);
      const union = unionOf(boxes);
      return union ? handlesFor(union, gizmosFor(null)) : [];
    }
    const id = [...this.selection][0];
    const box = this.boxes.get(id);
    if (!box) return [];
    const designer = this.o.designerFor ? this.o.designerFor(this.record(id)) : null;
    return handlesFor(box, gizmosFor(designer));
  }

  // --------------------------------------------------------------- pointer

  at(e) {
    const r = this.overlay.getBoundingClientRect();
    return this.viewport.toCanvas(e.clientX - r.left, e.clientY - r.top);
  }

  down(e) {
    if (e.button !== 0 || !e.isPrimary) return;
    const p = this.at(e);
    const handle = hitTest(this.handles(), p.x, p.y, grabRadius(this.viewport));
    try {
      this.overlay.setPointerCapture(e.pointerId);
    } catch {
      /* a browser that refuses capture still delivers move and up */
    }

    if (handle && this.selection.size) {
      this.beginDrag(e, p, handle);
      return;
    }
    const hit = this.itemAt(p.x, p.y);
    if (hit) {
      const additive = e.ctrlKey || e.metaKey || e.shiftKey;
      if (!this.selection.has(hit)) {
        this.selection = additive ? new Set([...this.selection, hit]) : new Set([hit]);
        this.announce();
      } else if (additive) {
        this.selection.delete(hit);
        this.announce();
      }
      this.beginDrag(e, p, { kind: "cage", action: "move", target: "transform.position", dir: [0, 0] });
      return;
    }
    // Empty space: a sweep, as in the tray.
    if (!(e.ctrlKey || e.metaKey)) {
      this.selection = new Set();
      this.announce();
    }
    this.state = { mode: "maybe-marquee", pointerId: e.pointerId, from: p, base: [...this.selection] };
  }

  /** The topmost item under a point, which is the last one drawn. */
  itemAt(x, y) {
    const entries = this.entries();
    for (let i = entries.length - 1; i >= 0; i -= 1) {
      if (inside(entries[i].box, x, y)) return entries[i].id;
    }
    return null;
  }

  beginDrag(e, p, handle) {
    const ids = [...this.selection];
    const starts = new Map();
    for (const id of ids) {
      const box = this.boxes.get(id);
      const record = this.record(id);
      if (box && record) starts.set(id, { box: Object.assign({}, box), transform: record.transform || {} });
    }
    this.state = {
      mode: "maybe-drag",
      pointerId: e.pointerId,
      from: p,
      client: { x: e.clientX, y: e.clientY },
      handle,
      starts,
      targets: null,
    };
    if (this.o.onGestureStart) this.o.onGestureStart(handle);
  }

  move(e) {
    const s = this.state;
    if (!s || e.pointerId !== s.pointerId) return;
    const p = this.at(e);

    if (s.mode === "maybe-drag") {
      if (Math.abs(e.clientX - s.client.x) + Math.abs(e.clientY - s.client.y) < THRESHOLD) return;
      s.mode = "drag";
      s.targets = snapTargets({
        canvas: this.viewport.canvas,
        boxes: this.entries().filter((x) => !this.selection.has(x.id)).map((x) => x.box),
        points: this.snapPoints(),
      });
      if (this.o.onDragStart) this.o.onDragStart();
    }
    if (s.mode === "maybe-marquee") {
      if (Math.abs(p.x - s.from.x) + Math.abs(p.y - s.from.y) < this.viewport.lengthFromSurface(THRESHOLD)) return;
      s.mode = "marquee";
    }

    if (s.mode === "marquee") {
      s.rect = rectFrom(s.from.x, s.from.y, p.x, p.y);
      const hits = this.entries().filter((entry) => overlaps(s.rect, entry.box)).map((entry) => entry.id);
      this.selection = new Set(e.ctrlKey || e.metaKey ? [...s.base, ...hits] : hits);
      this.announce();
      this.draw();
      e.preventDefault();
      return;
    }
    if (s.mode !== "drag") return;

    this.dragTo(p, e);
    e.preventDefault();
  }

  /** One frame of a drag: predict, draw, send. In that order, deliberately. */
  dragTo(p, e) {
    const s = this.state;
    let dx = p.x - s.from.x;
    let dy = p.y - s.from.y;
    this.guides = [];

    // Snapping applies to the move, on the union of what is being dragged, so
    // a selection of four lines up as one box rather than four arguments.
    if (this.snap && s.handle.action === "move") {
      const union = unionOf([...s.starts.values()].map((v) => v.box));
      if (union) {
        const moved = { x: union.x + dx, y: union.y + dy, width: union.width, height: union.height };
        const snapped = snapDelta(moved, s.targets, { invert: e.ctrlKey || e.metaKey });
        dx += snapped.dx;
        dy += snapped.dy;
        this.guides = snapped.guides;
      }
    }

    const mods = {
      aspect: e.shiftKey,
      centre: e.altKey,
      pointer: p,
      min: s.handle.min,
      max: s.handle.max,
      from: s.handle.from,
    };
    for (const [id, start] of s.starts) {
      const out = applyDrag(s.handle, start, dx, dy, mods);
      if (out.box) this.boxes.set(id, out.box);
      const seq = this.prediction.predict(id, out.props);
      this.send(id, out.props, seq);
    }
    this.paintNow();
  }

  /**
   * The same move, to the core, with its number.
   *
   * Not awaited: the drawing above has already happened, and the answer's only
   * job is to tell the prediction it can let go. The round trip is timed here
   * because that is the number 07 asks for.
   */
  send(id, props, seq) {
    const started = now();
    this.scenes
      .itemSet(this.scene, id, props, { seq, duration_ms: 0, draft: this.draft })
      .then((answer) => {
        this.timings.note("echo", now() - started);
        const done = this.prediction.settle(seq);
        // The answer is the draft's own view, so it is also how the composer
        // learns what the core made of the move. Handed over only once the
        // prediction has let go, or it would fight the hand.
        if (done.length && !this.prediction.busy) {
          if (this.o.onView) this.o.onView(answer);
          else this.draw();
        }
      })
      .catch((err) => {
        this.prediction.settle(seq);
        if (this.o.onError) this.o.onError(err);
      });
  }

  snapPoints() {
    const out = [];
    for (const record of this.items()) {
      if (this.selection.has(record.id)) continue;
      const box = this.boxes.get(record.id);
      const designer = this.o.designerFor ? this.o.designerFor(record) : null;
      if (box && designer && designer.snap) out.push(...pointsOf(record, box, designer.snap));
    }
    return out;
  }

  up(e) {
    const s = this.state;
    if (!s || e.pointerId !== s.pointerId) return;
    this.state = null;
    this.guides = [];
    try {
      this.overlay.releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }
    if (s.mode === "drag" && this.o.onDragEnd) this.o.onDragEnd();
    this.draw();
  }

  cancel() {
    this.state = null;
    this.guides = [];
    this.prediction.reset();
    this.draw();
  }

  announce() {
    if (this.o.onSelect) this.o.onSelect([...this.selection]);
    this.draw();
  }

  /** What the test page prints: the two numbers, with their samples. */
  report() {
    return this.timings.lines();
  }
}
