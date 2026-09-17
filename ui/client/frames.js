// Multiview frames off the wire and onto tiles.
//
// A frame from `/rpc` is a 16 byte header then JPEG:
//
//   offset 0   u32  seq              little endian
//   offset 4   u32  layout id        little endian
//   offset 8   u64  running time ns  little endian
//   offset 16  ...  JPEG bytes
//
// The layout id matches the id in `event/multiview.layout`, which is what lets
// a client cut cells out of a sheet without a race when the layout changes mid
// flight. Today's server sends a bare JPEG with no header; the legacy adapter
// hands frames here with `bare: true` and the layout the status document
// carries.
//
// One socket carries two pictures: the mosaic, and the armed scene for the
// pane beside the programme. The top bit of the sequence number says which,
// because the header had no spare field and every client reads it at sixteen
// bytes. A preview frame is one whole picture and names no grid.

export const HEADER_BYTES = 16;
export const PREVIEW_STREAM = 0x80000000;

/** Split one binary frame into its header and its JPEG. */
export function parseFrame(buffer) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : buffer;
  if (bytes.length <= HEADER_BYTES) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const seq = view.getUint32(0, true);
  return {
    seq: seq & ~PREVIEW_STREAM,
    preview: (seq & PREVIEW_STREAM) !== 0,
    layout: view.getUint32(4, true),
    runningTimeMs: view.getBigUint64(8, true),
    jpeg: bytes.subarray(HEADER_BYTES),
  };
}

/** Wrap a bare JPEG so callers see one shape whichever transport delivered it. */
export function bareFrame(buffer, layout) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : buffer;
  // The legacy stream is the mosaic and only ever was.
  return { seq: 0, preview: false, layout: layout || 0, runningTimeMs: 0n, jpeg: bytes };
}

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
    this.blobUrl = null;
  }

  /** The cell table from `event/multiview.layout` or from status.multiview. */
  setLayout(layout) {
    this.layout = layout;
  }

  /** Register a canvas to be painted with one cell. Returns a release function. */
  attach(canvas, cellIndex) {
    this.targets.set(canvas, cellIndex);
    this._paint();
    return () => this.targets.delete(canvas);
  }

  get wanted() {
    return this.targets.size > 0;
  }

  /** Take a frame. Decoding is async, so late frames are dropped, not queued. */
  async push(frame) {
    if (!this.wanted || this.pending) return;
    this.pending = true;
    try {
      const blob = new Blob([frame.jpeg], { type: "image/jpeg" });
      const decoded = await decode(blob);
      if (this.bitmap && this.bitmap.close) this.bitmap.close();
      this.bitmap = decoded;
      this._paint();
    } catch {
      // A truncated frame is not worth a message: the next one is along.
    } finally {
      this.pending = false;
    }
  }

  _paint() {
    if (!this.bitmap || !this.layout) return;
    const cells = this.layout.cells || [];
    for (const [canvas, index] of this.targets) {
      const cell = cells.find((c) => c.index === index);
      if (!cell) continue;
      const w = canvas.width;
      const h = canvas.height;
      if (!w || !h) continue;
      const ctx = canvas.getContext("2d", { alpha: false });
      if (!ctx) continue;
      ctx.drawImage(this.bitmap, cell.x, cell.y, cell.w, cell.h, 0, 0, w, h);
    }
  }

  destroy() {
    if (this.bitmap && this.bitmap.close) this.bitmap.close();
    this.bitmap = null;
    this.targets.clear();
  }
}

/**
 * The newest whole picture, painted onto every canvas attached to it.
 *
 * The preview is one frame rather than a sheet, so there is no layout to wait
 * for and nothing to cut out: decode, then blit. Same shape as `SheetPainter`
 * so a panel holds either the same way.
 */
export class PicturePainter {
  constructor() {
    this.bitmap = null;
    this.pending = false;
    this.targets = new Set();
  }

  /** Register a canvas to be painted with the whole picture. */
  attach(canvas) {
    this.targets.add(canvas);
    this._paint();
    return () => this.targets.delete(canvas);
  }

  get wanted() {
    return this.targets.size > 0;
  }

  /** Take a frame. Late frames are dropped rather than queued, as the sheet does. */
  async push(frame) {
    if (!this.wanted || this.pending) return;
    this.pending = true;
    try {
      const decoded = await decode(new Blob([frame.jpeg], { type: "image/jpeg" }));
      if (this.bitmap && this.bitmap.close) this.bitmap.close();
      this.bitmap = decoded;
      this._paint();
    } catch {
      // A truncated frame is not worth a message: the next one is along.
    } finally {
      this.pending = false;
    }
  }

  _paint() {
    if (!this.bitmap) return;
    for (const canvas of this.targets) {
      if (!canvas.width || !canvas.height) continue;
      const ctx = canvas.getContext("2d", { alpha: false });
      if (ctx) ctx.drawImage(this.bitmap, 0, 0, canvas.width, canvas.height);
    }
  }

  destroy() {
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

/**
 * The width to ask the core for.
 *
 * The mosaic is one sheet holding `cols` cells across. A tile is `cssWidth`
 * device independent pixels wide on a screen with `devicePixelRatio` real
 * pixels per one of those. Asking for more than that is bytes nobody sees;
 * asking for less is a soft picture. Rounded up to a multiple of 16 because
 * encoders like even macroblocks, and clamped to the range the protocol allows.
 */
export function sheetWidthFor(cssWidth, cols, dpr) {
  const ratio = dpr || (typeof devicePixelRatio === "number" ? devicePixelRatio : 1);
  const perCell = Math.ceil(cssWidth * ratio);
  const sheet = perCell * Math.max(1, cols || 1);
  const rounded = Math.ceil(sheet / 16) * 16;
  return Math.max(320, Math.min(1920, rounded));
}
