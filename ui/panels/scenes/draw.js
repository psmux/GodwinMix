// One scene, drawn onto a canvas out of the multiview sheet's cells.
//
// The boxes are the core's (`geometry` on a scene view, in the order the
// compositor stacks them) and the rest is the item's own record: its crop, how
// it fits its box, where it sits in it, its turn. A source the mixer does not
// have has no cell and is left out, which is what the programme does with it.
// Graphics and text have no cell either and are left out; the picture says so
// by not pretending.

const ALIGN = { left: 0, center: 0.5, right: 1, top: 0, bottom: 1 };

/** "top-right" as fractions across and down; "center" is the middle. */
export function alignOf(word) {
  const [v, h] = String(word || "center").split("-");
  if (!h) return { ax: 0.5, ay: 0.5 };
  return { ax: ALIGN[h] ?? 0.5, ay: ALIGN[v] ?? 0.5 };
}

/**
 * Where a cropped source goes in its frame, as drawImage wants it:
 * [sx, sy, sw, sh, dx, dy, dw, dh]. Everything in one space, the sheet's for
 * the source and the canvas's for the frame.
 */
export function placement(src, frame, fit, align) {
  const { ax, ay } = alignOf(align);
  let scale;
  if (fit === "cover") scale = Math.max(frame.w / src.w, frame.h / src.h);
  else if (fit === "contain" || fit === "max") scale = Math.min(frame.w / src.w, frame.h / src.h);
  else if (fit === "fit-width") scale = frame.w / src.w;
  else if (fit === "fit-height") scale = frame.h / src.h;
  else return [src.x, src.y, src.w, src.h, frame.x, frame.y, frame.w, frame.h];
  const dw = src.w * scale;
  const dh = src.h * scale;
  if (fit !== "cover") return [src.x, src.y, src.w, src.h, frame.x + (frame.w - dw) * ax, frame.y + (frame.h - dh) * ay, dw, dh];
  // Cover crops the overflow: take the part of the source that fills the frame.
  const sw = frame.w / scale;
  const sh = frame.h / scale;
  return [src.x + (src.w - sw) * ax, src.y + (src.h - sh) * ay, sw, sh, frame.x, frame.y, frame.w, frame.h];
}

/** A cell with the record's crop taken off, as fractions of its own size. */
function cropped(cell, crop) {
  const c = crop || {};
  const l = c.left || 0, r = c.right || 0, t = c.top || 0, b = c.bottom || 0;
  return { x: cell.x + cell.w * l, y: cell.y + cell.h * t, w: cell.w * Math.max(0.01, 1 - l - r), h: cell.h * Math.max(0.01, 1 - t - b) };
}

export function drawScene(canvas, view, mirror, cells, bitmap) {
  const ctx = canvas.getContext("2d", { alpha: false });
  if (!ctx) return;
  const W = canvas.width;
  const H = canvas.height;
  ctx.globalAlpha = 1;
  ctx.fillStyle = "#000";
  ctx.fillRect(0, 0, W, H);
  const sx = W / ((view.canvas && view.canvas.width) || 1920);
  const sy = H / ((view.canvas && view.canvas.height) || 1080);
  for (const box of view.geometry || []) {
    const cell = box.source && cells.get(box.source);
    const record = cell && mirror.record(box.item);
    if (!record || record.visible === false) continue;
    const t = record.transform || {};
    const frame = { x: box.x * sx, y: box.y * sy, w: box.width * sx, h: box.height * sy };
    const at = placement(cropped(cell, record.crop), frame, t.fit, t.align);
    ctx.save();
    ctx.globalAlpha = Math.max(0, Math.min(1, box.opacity ?? 1));
    if (t.rotation) {
      ctx.translate(frame.x + frame.w / 2, frame.y + frame.h / 2);
      ctx.rotate((t.rotation * Math.PI) / 180);
      ctx.translate(-(frame.x + frame.w / 2), -(frame.y + frame.h / 2));
    }
    ctx.drawImage(bitmap, ...at);
    ctx.restore();
  }
}
