// Where a pointer on the Interact view lands on the page, and which key
// presses go to it. No DOM in here, so ui/test reaches all of it.
//
// The picture is shown with object-fit: contain, so it can have bars down the
// sides or across the top. A point is measured from the picture itself, not
// from the box around it, and sent as a share of the page from 0 to 1. The
// renderer turns the share into its own pixels, whatever size it draws at.

/** The rectangle the picture fills inside a box, after letterboxing. */
export function contentBox(boxW, boxH, picW, picH) {
  if (!(boxW > 0 && boxH > 0)) return { x: 0, y: 0, w: 0, h: 0 };
  if (!(picW > 0 && picH > 0)) return { x: 0, y: 0, w: boxW, h: boxH };
  const scale = Math.min(boxW / picW, boxH / picH);
  const w = picW * scale;
  const h = picH * scale;
  return { x: (boxW - w) / 2, y: (boxH - h) / 2, w, h };
}

const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);

/**
 * A point in the box (its own pixels, from its top left) as a share of the
 * page, and whether it was on the picture at all. Off the picture is pinned
 * to its edge so a drag that runs over the bars still ends somewhere.
 */
export function toShare(px, py, boxW, boxH, picW, picH) {
  const c = contentBox(boxW, boxH, picW, picH);
  if (!(c.w > 0 && c.h > 0)) return { x: 0, y: 0, inside: false };
  const x = (px - c.x) / c.w;
  const y = (py - c.y) / c.h;
  const inside = x >= 0 && x <= 1 && y >= 0 && y <= 1;
  return { x: round(clamp01(x)), y: round(clamp01(y)), inside };
}

const round = (v) => Math.round(v * 10000) / 10000;

/** How far the page is from the picture: page pixels per pixel on screen. */
export function pageScale(picW, shownW) {
  return picW > 0 && shownW > 0 ? picW / shownW : 1;
}

/** A wheel event's movement in pixels, whatever unit it came in. */
export function wheelPixels(deltaX, deltaY, deltaMode) {
  const unit = deltaMode === 1 ? 40 : deltaMode === 2 ? 800 : 1;
  return { dx: Math.round(deltaX * unit), dy: Math.round(deltaY * unit) };
}

/** Keys with no character that a page still needs: editing, moving, leaving. */
const NAMED = new Set([
  "Enter", "Tab", "Backspace", "Delete", "Escape", "ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown",
  "Home", "End", "PageUp", "PageDown", "Insert", " ",
  "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12",
]);

/**
 * The `key` event to send for a keydown, or null for one to leave alone: a
 * key held down by itself (Shift), and a press in the middle of an input
 * method's composition, which arrives as text when it is done.
 */
export function keyFor(e) {
  if (e.isComposing || e.keyCode === 229) return null;
  const printable = typeof e.key === "string" && [...e.key].length === 1;
  if (!printable && !NAMED.has(e.key)) return null;
  const out = { type: "key", code: e.keyCode | 0 };
  if (printable) out.key = e.key;
  for (const m of ["shift", "ctrl", "alt", "meta"]) if (e[`${m}Key`]) out[m] = true;
  return out;
}

/** Two presses this close in time and place are a double click. */
export function clickCount(last, now) {
  if (!last) return 1;
  const near = Math.abs(now.x - last.x) < 6 && Math.abs(now.y - last.y) < 6;
  return near && now.t - last.t < 400 ? Math.min(3, last.count + 1) : 1;
}

/**
 * Pointer moves since the last send, kept as the newest only, with every
 * other event in order. What the view sends while a call is in flight.
 */
export function coalesce(queue, event) {
  const last = queue[queue.length - 1];
  if (event.type === "move" && last && last.type === "move") queue[queue.length - 1] = event;
  else queue.push(event);
  return queue;
}
