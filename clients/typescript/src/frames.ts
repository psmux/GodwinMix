// Multiview frames off the wire.
//
// A binary frame on `/rpc` is a 16 byte header then JPEG:
//
//   offset 0   u32  seq               little endian
//   offset 4   u32  layout id         little endian
//   offset 8   u64  running time ms   little endian
//   offset 16  ...  JPEG bytes
//
// The layout id matches the id in `event/multiview.layout`, which is what lets
// a client cut cells out of a sheet without a race when the layout changes mid
// flight. The core's writer is `api::rpc::frame_header` in the Rust source and
// the third field is milliseconds there, so it is milliseconds here.

import type { CellAssignment, MultiviewLayout } from "./generated/protocol.ts";

export const HEADER_BYTES = 16;

export interface Frame {
  /** Counts up per connection, and wraps rather than stopping. */
  seq: number;
  /** Which `event/multiview.layout` this picture was cut for. */
  layout: number;
  /** Programme running time the frame was taken at, in milliseconds. */
  runningTimeMs: number;
  /** Still compressed. Decode it once for the sheet, not once per tile. */
  jpeg: Uint8Array;
}

/** Split one binary message into its header and its JPEG, or null if it is neither. */
export function parseFrame(buffer: ArrayBuffer | Uint8Array): Frame | null {
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  if (bytes.length <= HEADER_BYTES) return null;
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  return {
    seq: view.getUint32(0, true),
    layout: view.getUint32(4, true),
    // A running time in milliseconds passes 2^53 after 285,000 years, so a
    // Number is safe here and saves every caller a BigInt.
    runningTimeMs: Number(view.getBigUint64(8, true)),
    jpeg: bytes.subarray(HEADER_BYTES),
  };
}

/** Wrap a bare JPEG so callers see one shape whichever core delivered it. */
export function bareFrame(buffer: ArrayBuffer | Uint8Array, layout = 0): Frame {
  const bytes = buffer instanceof Uint8Array ? buffer : new Uint8Array(buffer);
  return { seq: 0, layout, runningTimeMs: 0, jpeg: bytes };
}

/**
 * The cell one source sits in, for a frame and the layout it names.
 *
 * Returns null when the frame belongs to a different layout, which is the case
 * a client hits for a frame or two after the grid changes. Painting the old
 * rectangle out of the new sheet is how a UI shows the wrong camera.
 */
export function cellFor(
  frame: Frame,
  layout: MultiviewLayout | null | undefined,
  source: string,
): CellAssignment | null {
  if (!layout || layout.id !== frame.layout) return null;
  return (layout.cells || []).find((c) => c.source === source) || null;
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
export function sheetWidthFor(cssWidth: number, cols: number, dpr?: number): number {
  const ratio = dpr || (typeof devicePixelRatio === "number" ? devicePixelRatio : 1);
  const perCell = Math.ceil(cssWidth * ratio);
  const sheet = perCell * Math.max(1, cols || 1);
  const rounded = Math.ceil(sheet / 16) * 16;
  return Math.max(320, Math.min(1920, rounded));
}
