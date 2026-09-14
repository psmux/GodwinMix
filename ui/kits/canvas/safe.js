// Safe areas and rulers.
//
// The same two numbers the core's validator uses (`validate.rs`): action safe
// is the middle 93 percent, title safe the middle 90. Drawing them with
// different numbers from the ones `scene.validate` warns about would be worse
// than not drawing them at all, so they are named here once and the fixtures
// check the ports against the same pair.

export const ACTION_SAFE = 0.07;
export const TITLE_SAFE = 0.1;

/** Shrink a box towards its centre by a fraction of each side. */
export function insetFraction(box, fraction) {
  const dx = (box.width * fraction) / 2;
  const dy = (box.height * fraction) / 2;
  return { x: box.x + dx, y: box.y + dy, width: box.width - dx * 2, height: box.height - dy * 2 };
}

/** The two rectangles a designer draws over the picture, in canvas pixels. */
export function safeAreas(canvas) {
  const full = { x: 0, y: 0, width: canvas.width, height: canvas.height };
  return {
    action: insetFraction(full, ACTION_SAFE),
    title: insetFraction(full, TITLE_SAFE),
  };
}

/**
 * Ruler ticks along one axis, at a spacing a person reads rather than a round
 * number of pixels: about one tick per 80 screen pixels, rounded to 10, 20, 50,
 * 100 and so on, so the numbers stay legible at any zoom.
 */
export function ticks(length, scale, wanted = 80) {
  const raw = wanted / (scale || 1);
  const power = Math.pow(10, Math.floor(Math.log10(Math.max(1, raw))));
  const step = [1, 2, 5, 10].map((m) => m * power).find((s) => s >= raw) || power * 10;
  const out = [];
  for (let at = 0; at <= length + 0.5; at += step) out.push(Math.round(at));
  return out;
}
