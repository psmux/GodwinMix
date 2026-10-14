// Decoded pictures onto canvases: the mosaic sheet cut into cells, and the
// armed scene's preview whole.
//
// Both paint on the screen's next refresh and never more than once a refresh
// (next-frame.js says why), and neither draws a canvas again with what it
// already shows. Its own module, so frames.js stays about the wire.

import { nextFrame } from "./next-frame.js";

/**
 * Holds the newest sheet as one ImageBitmap and paints cells out of it.
 *
 * One decode per frame for the whole mosaic, then a canvas blit per visible
 * tile. Decoding once and blitting many is the difference between a Pi keeping
 * up and not. `createImageBitmap` is present in WebView2, WebKitGTK and Safari
 * 15 and later, so all three shells take the same path.
 */
export class SheetPainter {
  constructor() {
    this.bitmap = null;
    this.layout = null;
    this.pending = false;
    this.targets = new Map(); // canvas -> cell index
    this.watchers = new Set(); // fn(bitmap, layout), for pictures made of several cells
    // Which sheet and layout each canvas was last painted from, so drawing it
    // again with the same picture is skipped.
    this.painted = new WeakMap();
    this.sheets = 0;
    this.layouts = 0;
    this.handed = 0;
    this.frame = nextFrame(() => this._paint());
  }

  /** The cell table from `event/multiview.layout` or from status.multiview. */
  setLayout(layout) {
    this.layout = layout;
    this.layouts++;
  }

  /**
   * Register a canvas to be painted with one cell. Returns a release function.
   *
   * Paints that canvas alone, and only when it does not already hold this
   * sheet's cell: a new canvas, a resized one, or another cell. The Sources
   * panel attaches every tile again each time it renders, several times a
   * frame, and repainting on each attach drew every tile about eighteen times
   * a second for the three new pictures that had arrived.
   */
  attach(canvas, cellIndex) {
    this.targets.set(canvas, cellIndex);
    if (this.bitmap && this.layout) this._paintOne(canvas, cellIndex, this.layout.cells || []);
    return () => this.targets.delete(canvas);
  }

  /**
   * Be handed every decoded sheet, for a picture put together from several
   * cells (a scene's). Returns a release function.
   */
  observe(fn) {
    this.watchers.add(fn);
    return () => this.watchers.delete(fn);
  }

  get wanted() {
    return this.targets.size > 0 || this.watchers.size > 0;
  }

  /**
   * Take a frame. Decoding is async, so late frames are dropped, not queued,
   * and so is a frame arriving while a decoded one still waits for the screen.
   */
  async push(frame) {
    if (!this.wanted || this.pending || this.frame.pending) return;
    this.pending = true;
    try {
      const blob = new Blob([frame.jpeg], { type: "image/jpeg" });
      const decoded = await decode(blob);
      if (this.bitmap && this.bitmap.close) this.bitmap.close();
      this.bitmap = decoded;
      this.sheets++;
      this.frame.request();
    } catch {
      // A truncated frame is not worth a message: the next one is along.
    } finally {
      this.pending = false;
    }
  }

  _paint() {
    if (!this.bitmap || !this.layout) return;
    const cells = this.layout.cells || [];
    for (const [canvas, index] of this.targets) this._paintOne(canvas, index, cells);
    if (this.handed === this.sheets) return;
    this.handed = this.sheets;
    for (const fn of this.watchers) fn(this.bitmap, this.layout);
  }

  _paintOne(canvas, index, cells) {
    const w = canvas.width;
    const h = canvas.height;
    const stamp = `${this.sheets}/${this.layouts}/${index}/${w}x${h}`;
    if (this.painted.get(canvas) === stamp) return;
    const cell = cells.find((c) => c.index === index);
    if (!cell || !w || !h) return;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;
    ctx.drawImage(this.bitmap, cell.x, cell.y, cell.w, cell.h, 0, 0, w, h);
    this.painted.set(canvas, stamp);
  }

  destroy() {
    this.frame.cancel();
    if (this.bitmap && this.bitmap.close) this.bitmap.close();
    this.bitmap = null;
    this.targets.clear();
    this.watchers.clear();
  }
}

/**
 * The newest whole picture, painted onto every canvas attached to it.
 *
 * The preview is one frame rather than a sheet, so there is no layout to wait
 * for and nothing to cut out: decode, then blit. Same shape as `SheetPainter`.
 */
export class PicturePainter {
  constructor() {
    this.bitmap = null;
    this.pending = false;
    this.targets = new Set();
    this.painted = new WeakMap();
    this.pictures = 0;
    this.frame = nextFrame(() => this._paint());
  }

  /** Register a canvas to be painted with the whole picture. Paints that one alone. */
  attach(canvas) {
    this.targets.add(canvas);
    this._paintOne(canvas);
    return () => this.targets.delete(canvas);
  }

  get wanted() {
    return this.targets.size > 0;
  }

  /** Take a frame. Late frames are dropped rather than queued, as the sheet does. */
  async push(frame) {
    if (!this.wanted || this.pending || this.frame.pending) return;
    this.pending = true;
    try {
      const decoded = await decode(new Blob([frame.jpeg], { type: "image/jpeg" }));
      if (this.bitmap && this.bitmap.close) this.bitmap.close();
      this.bitmap = decoded;
      this.pictures++;
      this.frame.request();
    } catch {
      // A truncated frame is not worth a message: the next one is along.
    } finally {
      this.pending = false;
    }
  }

  _paint() {
    for (const canvas of this.targets) this._paintOne(canvas);
  }

  _paintOne(canvas) {
    const w = canvas.width;
    const h = canvas.height;
    const stamp = `${this.pictures}/${w}x${h}`;
    if (!this.bitmap || !w || !h || this.painted.get(canvas) === stamp) return;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;
    ctx.drawImage(this.bitmap, 0, 0, w, h);
    this.painted.set(canvas, stamp);
  }

  destroy() {
    this.frame.cancel();
    if (this.bitmap && this.bitmap.close) this.bitmap.close();
    this.bitmap = null;
    this.targets.clear();
  }
}

/**
 * One JPEG to something `drawImage` accepts.
 *
 * `createImageBitmap` is the one to use: it decodes off the main thread and the
 * result can be closed, so a wall of tiles at eight frames a second does not
 * leave a hundred decoded pictures for the collector. WebKit before Safari 15
 * has no such thing, and the Tauri window on an older macOS is exactly that,
 * so an <img> over an object URL stands in. Same interface to the caller.
 */
function decode(blob) {
  if (typeof createImageBitmap === "function") return createImageBitmap(blob);
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(blob);
    const img = new Image();
    img.onload = () => {
      // Revoking inside onload, never before: revoking early blanks the image
      // on some builds and leaks one URL per frame on the rest.
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = () => {
      URL.revokeObjectURL(url);
      reject(new Error("the frame did not decode"));
    };
    img.src = url;
  });
}
