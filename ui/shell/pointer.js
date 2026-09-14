// One pointer pipeline for selecting, sweeping and dragging tiles.
//
// Pointer events, never HTML5 drag and drop. The reason is Windows: WebView2
// swallows in page `dragstart` and `drop` when the window also accepts files
// dropped from the desktop, which this window does, because media upload is a
// drop zone. A `dragstart` implementation works on macOS and quietly does
// nothing on Windows. Pointer events behave the same in WebView2, in WebKitGTK
// and in Safari, and they are also the only model that works under a finger.
//
// HTML5 drag and drop is still used for one thing and one thing only: files
// arriving from outside the page, on `window`. That path is the operating
// system's, not ours, and it is unaffected.

import { Selection, overlaps, rectFrom } from "./selection.js";
import { el, on } from "./dom.js";

/** Travel in CSS pixels before a press becomes a drag rather than a click. */
export const DRAG_THRESHOLD = 5;

export class DragSelect {
  /**
   * @param {{
   *   container: HTMLElement,
   *   selection: Selection,
   *   order: () => string[],
   *   onChange: () => void,
   *   onActivate?: (id: string, ev: PointerEvent) => void,
   *   onOpen?: (id: string) => void,
   *   onDrop?: (info: object) => void,
   *   onMenu?: (id: string|null, ev: MouseEvent) => void
   * }} opts
   */
  constructor(opts) {
    this.o = opts;
    this.sel = opts.selection || new Selection();
    this.state = null;
    this.offs = [
      on(opts.container, "pointerdown", (e) => this._down(e)),
      on(opts.container, "pointermove", (e) => this._move(e)),
      on(opts.container, "pointerup", (e) => this._up(e)),
      on(opts.container, "pointercancel", () => this._cancel()),
      on(opts.container, "dblclick", (e) => this._dbl(e)),
      on(opts.container, "contextmenu", (e) => this._menu(e)),
      on(window, "keydown", (e) => {
        if (e.key === "Escape" && this.state) {
          e.preventDefault();
          this._cancel();
        }
      }),
    ];
  }

  destroy() {
    for (const off of this.offs) off();
    this.offs = [];
  }

  /** The tile an event landed on, or null for empty space. */
  _tile(target) {
    const node = target && target.closest ? target.closest("[data-id]") : null;
    return node && this.o.container.contains(node) ? node : null;
  }

  /** Controls inside a tile keep their own behaviour: a fader is not a drag. */
  _inert(target) {
    return !!(target && target.closest && target.closest("input, button, select, textarea, a, [data-nodrag]"));
  }

  _down(e) {
    if (e.button !== 0 || !e.isPrimary) return;
    if (this._inert(e.target)) return;
    const tile = this._tile(e.target);
    const order = this.o.order();
    const mods = { toggle: e.ctrlKey || e.metaKey, extend: e.shiftKey };

    // Capture on the container, so a pointer that leaves the window mid sweep
    // still delivers its move and up. Without this a drag that ends over the
    // desktop leaves the marquee painted.
    try {
      this.o.container.setPointerCapture(e.pointerId);
    } catch {
      /* a browser that refuses capture still works, just less tidily */
    }

    if (tile) {
      const id = tile.dataset.id;
      const deferred = this.sel.press(id, order, mods);
      this.state = {
        mode: "press",
        id,
        deferred,
        startX: e.clientX,
        startY: e.clientY,
        pointerId: e.pointerId,
        before: [...this.sel.ids],
        beforeAnchor: this.sel.anchor,
        copy: false,
      };
      this.o.onChange();
      return;
    }

    // Empty space: a sweep. Ctrl keeps what is already chosen.
    const base = mods.toggle ? [...this.sel.ids] : [];
    if (!mods.toggle && this.sel.size) {
      this.sel.clear();
      this.o.onChange();
    }
    this.state = {
      mode: "maybe-marquee",
      startX: e.clientX,
      startY: e.clientY,
      pointerId: e.pointerId,
      base,
      additive: mods.toggle,
      before: [...this.sel.ids],
      beforeAnchor: this.sel.anchor,
    };
  }

  _move(e) {
    const s = this.state;
    if (!s || e.pointerId !== s.pointerId) return;
    const dx = e.clientX - s.startX;
    const dy = e.clientY - s.startY;
    const far = Math.abs(dx) >= DRAG_THRESHOLD || Math.abs(dy) >= DRAG_THRESHOLD;

    if (s.mode === "press" && far) {
      s.mode = "drag";
      s.ghost = this._ghost();
      this.o.container.classList.add("dragging-tiles");
      for (const id of this.sel.ids) {
        const node = this._node(id);
        if (node) node.classList.add("dragging");
      }
    }
    if (s.mode === "maybe-marquee" && far) {
      s.mode = "marquee";
      s.box = el("div.marquee");
      this.o.container.appendChild(s.box);
    }

    if (s.mode === "drag") {
      s.copy = e.altKey || (e.ctrlKey && !e.metaKey && navigator.platform.indexOf("Mac") < 0);
      s.ghost.style.transform = `translate(${e.clientX + 12}px, ${e.clientY + 12}px)`;
      this._hover(e.clientX, e.clientY, s);
      e.preventDefault();
      return;
    }

    if (s.mode === "marquee") {
      const box = this.o.container.getBoundingClientRect();
      const rect = rectFrom(s.startX, s.startY, e.clientX, e.clientY);
      s.box.style.left = rect.x - box.left + this.o.container.scrollLeft + "px";
      s.box.style.top = rect.y - box.top + this.o.container.scrollTop + "px";
      s.box.style.width = rect.w + "px";
      s.box.style.height = rect.h + "px";
      const hits = [];
      for (const node of this.o.container.querySelectorAll("[data-id]")) {
        const r = node.getBoundingClientRect();
        if (overlaps(rect, { x: r.left, y: r.top, w: r.width, h: r.height })) hits.push(node.dataset.id);
      }
      this.sel.marquee(hits, s.additive, s.base);
      this.o.onChange();
      e.preventDefault();
    }
  }

  _up(e) {
    const s = this.state;
    if (!s || e.pointerId !== s.pointerId) return;
    this.state = null;
    try {
      this.o.container.releasePointerCapture(e.pointerId);
    } catch {
      /* already released */
    }

    if (s.mode === "press") {
      if (this.sel.release(s.deferred)) this.o.onChange();
      if (this.o.onActivate) this.o.onActivate(s.id, e);
      return;
    }
    if (s.mode === "maybe-marquee") {
      // A click on empty space clears the selection and nothing else.
      return;
    }
    if (s.mode === "marquee") {
      s.box.remove();
      return;
    }
    if (s.mode === "drag") {
      this._endDrag(s);
      const target = this._dropTarget(e.clientX, e.clientY);
      if (this.o.onDrop) {
        this.o.onDrop({
          ids: this.sel.list(this.o.order()),
          target: target ? target.dataset.drop : null,
          targetId: target && target.dataset.id ? target.dataset.id : null,
          copy: s.copy,
          x: e.clientX,
          y: e.clientY,
        });
      }
    }
  }

  _cancel() {
    const s = this.state;
    this.state = null;
    if (!s) return;
    if (s.box) s.box.remove();
    if (s.mode === "drag") this._endDrag(s);
    // Escape restores the selection the gesture started from.
    this.sel.set(s.before, s.beforeAnchor);
    this.o.onChange();
  }

  _endDrag(s) {
    if (s.ghost) s.ghost.remove();
    this.o.container.classList.remove("dragging-tiles");
    for (const node of this.o.container.querySelectorAll(".dragging")) node.classList.remove("dragging");
    for (const node of this.o.container.querySelectorAll(".drop-into")) node.classList.remove("drop-into");
  }

  _ghost() {
    const n = this.sel.size;
    const ghost = el("div.toast", { text: n === 1 ? "Moving 1" : `Moving ${n}` });
    Object.assign(ghost.style, { position: "fixed", left: "0", top: "0", zIndex: "90", pointerEvents: "none" });
    document.body.appendChild(ghost);
    return ghost;
  }

  _node(id) {
    return this.o.container.querySelector(`[data-id="${cssEscape(id)}"]`);
  }

  _dropTarget(x, y) {
    const under = document.elementFromPoint(x, y);
    return under && under.closest ? under.closest("[data-drop]") : null;
  }

  _hover(x, y, s) {
    const target = this._dropTarget(x, y);
    for (const node of this.o.container.querySelectorAll(".drop-into")) {
      if (node !== target) node.classList.remove("drop-into");
    }
    if (target && !this.sel.has(target.dataset.id)) target.classList.add("drop-into");
    s.hover = target;
  }

  _dbl(e) {
    const tile = this._tile(e.target);
    if (tile && this.o.onOpen && !this._inert(e.target)) this.o.onOpen(tile.dataset.id);
  }

  _menu(e) {
    if (!this.o.onMenu) return;
    const tile = this._tile(e.target);
    if (tile && !this.sel.has(tile.dataset.id)) {
      this.sel.click(tile.dataset.id, this.o.order(), {});
      this.o.onChange();
    }
    e.preventDefault();
    this.o.onMenu(tile ? tile.dataset.id : null, e);
  }
}

/** CSS.escape is missing from older WebKitGTK, so this stands in for ids. */
function cssEscape(s) {
  if (typeof CSS !== "undefined" && CSS.escape) return CSS.escape(s);
  return String(s).replace(/["\\]/g, "\\$&");
}
