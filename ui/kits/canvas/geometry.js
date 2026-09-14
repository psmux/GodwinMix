// The one piece of scene arithmetic a client is allowed to do for itself.
//
// Everything else comes from the core: every mutating command answers with the
// flattened geometry, so a client never recomputes a layout and never disagrees
// with the compositor about where an item is. What a client does need locally
// is the reverse map, because a drag produces a rectangle and the document
// wants a transform:
//
//     x = position.x - anchor.x * (frame.w * scale.x)
//
// That is `geometry.rs::item_rect`, written out once here and ported with the
// fixtures, which is why the handles land on the picture in all three kits.

/** The box an item occupies, before rotation, from its transform. */
export function itemRect(transform, canvas) {
  const t = transform || {};
  const frame = t.frame || { w: canvas.width, h: canvas.height };
  const scale = t.scale || { x: 1, y: 1 };
  const anchor = t.anchor || { x: 0, y: 0 };
  const w = n(frame.w, canvas.width) * n(scale.x, 1);
  const h = n(frame.h, canvas.height) * n(scale.y, 1);
  return {
    x: n(t.position && t.position.x) - n(anchor.x) * w,
    y: n(t.position && t.position.y) - n(anchor.y) * h,
    width: w,
    height: h,
  };
}

/** The transform a rectangle asks for, keeping the item's anchor and scale. */
export function rectToTransform(rect, transform) {
  const t = transform || {};
  const scale = t.scale || { x: 1, y: 1 };
  const anchor = t.anchor || { x: 0, y: 0 };
  return {
    frame: { w: rect.width / (n(scale.x, 1) || 1), h: rect.height / (n(scale.y, 1) || 1) },
    position: { x: rect.x + n(anchor.x) * rect.width, y: rect.y + n(anchor.y) * rect.height },
  };
}

/** Do two boxes touch? Both are `{x, y, width, height}`. */
export function overlaps(a, b) {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** Is a point inside a box? */
export function inside(box, x, y) {
  return x >= box.x && x <= box.x + box.width && y >= box.y && y <= box.y + box.height;
}

/** Normalise a sweep from its two corners, so dragging up and left works. */
export function rectFrom(x0, y0, x1, y1) {
  return {
    x: Math.min(x0, x1),
    y: Math.min(y0, y1),
    width: Math.abs(x1 - x0),
    height: Math.abs(y1 - y0),
  };
}

/** The smallest box round a set of boxes, or null for none. */
export function unionOf(boxes) {
  if (!boxes || !boxes.length) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const b of boxes) {
    x0 = Math.min(x0, b.x);
    y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.width);
    y1 = Math.max(y1, b.y + b.height);
  }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/**
 * Canvas pixels to the surface the preview is drawn on, and back.
 *
 * The preview picture is whatever size the window gave it; the document is in
 * canvas pixels. One object holds the two, so nothing else in the composer
 * multiplies by a ratio and gets it wrong in one place out of five.
 */
export class View {
  constructor(canvas, surface) {
    this.set(canvas, surface);
  }

  set(canvas, surface) {
    this.canvas = canvas || { width: 1920, height: 1080 };
    this.surface = surface || { width: 1, height: 1 };
    // The picture is letterboxed inside the surface, the way `object-fit:
    // contain` draws it, so the handles sit on the picture and not beside it.
    const scale = Math.min(this.surface.width / this.canvas.width, this.surface.height / this.canvas.height);
    this.scale = isFinite(scale) && scale > 0 ? scale : 1;
    this.offsetX = (this.surface.width - this.canvas.width * this.scale) / 2;
    this.offsetY = (this.surface.height - this.canvas.height * this.scale) / 2;
    return this;
  }

  toSurface(x, y) {
    return { x: this.offsetX + x * this.scale, y: this.offsetY + y * this.scale };
  }

  toCanvas(x, y) {
    return { x: (x - this.offsetX) / this.scale, y: (y - this.offsetY) / this.scale };
  }

  boxToSurface(box) {
    const p = this.toSurface(box.x, box.y);
    return { x: p.x, y: p.y, width: box.width * this.scale, height: box.height * this.scale };
  }

  /** A length in canvas pixels that draws as `px` on the surface. */
  lengthFromSurface(px) {
    return px / this.scale;
  }
}

function n(v, fallback) {
  return typeof v === "number" && isFinite(v) ? v : fallback === undefined ? 0 : fallback;
}
