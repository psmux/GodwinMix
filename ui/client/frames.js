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
// One socket carries up to three pictures: the mosaic, the armed scene for
// the pane beside the programme, and the programme itself at full quality for
// a full screen output (`ext.programme`). The top bit of the sequence number
// marks a single picture, because the header had no spare field; a single
// picture names no grid, so its layout field says which one it is: 0 the
// armed scene, 1 the programme.

export const HEADER_BYTES = 16;
export const PREVIEW_STREAM = 0x80000000;
export const PROGRAMME_PICTURE = 1;

/** Split one binary frame into its header and its JPEG. */
export function parseFrame(buffer) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : buffer;
  if (bytes.length <= HEADER_BYTES) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const seq = view.getUint32(0, true);
  const single = (seq & PREVIEW_STREAM) !== 0;
  const layout = view.getUint32(4, true);
  const programme = single && layout === PROGRAMME_PICTURE;
  return {
    seq: seq & ~PREVIEW_STREAM,
    preview: single && !programme,
    programme,
    layout,
    runningTimeMs: view.getBigUint64(8, true),
    jpeg: bytes.subarray(HEADER_BYTES),
  };
}

/** Wrap a bare JPEG so callers see one shape whichever transport delivered it. */
export function bareFrame(buffer, layout) {
  const bytes = buffer instanceof ArrayBuffer ? new Uint8Array(buffer) : buffer;
  // The legacy stream is the mosaic and only ever was.
  return { seq: 0, preview: false, programme: false, layout: layout || 0, runningTimeMs: 0n, jpeg: bytes };
}

// The painters are in painters.js, and are exported from here as they always were.
export { SheetPainter, PicturePainter } from "./painters.js";

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
