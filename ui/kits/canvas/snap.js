// Snapping, split the way tldraw splits it (11 section 5).
//
// The plugin answers with geometry: `snap = { bounds = true, points =
// ["params.anchor_point"] }` says this item type snaps by its box and by one
// named point of its own. The client owns everything else: the threshold, the
// modifier that turns snapping off, the grid, and the drawing of the guides.
// That division is why a plugin author never tunes a pixel and why a Pi can use
// a wider threshold than a workstation without asking anybody.

/** How close, in canvas pixels, before an edge is taken as lined up. */
export const THRESHOLD = 8;

/**
 * Every line a moving box could line up with.
 *
 * @param {{canvas: object, boxes: object[], points?: {x,y}[], grid?: number}} what
 * @returns {{v: {at: number, span: [number, number]}[], h: same}}
 */
export function snapTargets(what) {
  const v = [];
  const h = [];
  const canvas = what.canvas;
  if (canvas) {
    // The canvas edges and its two centre lines, which is what most items are
    // actually being lined up against.
    push(v, 0, [0, canvas.height]);
    push(v, canvas.width / 2, [0, canvas.height]);
    push(v, canvas.width, [0, canvas.height]);
    push(h, 0, [0, canvas.width]);
    push(h, canvas.height / 2, [0, canvas.width]);
    push(h, canvas.height, [0, canvas.width]);
  }
  for (const box of what.boxes || []) {
    const span = [box.y, box.y + box.height];
    push(v, box.x, span);
    push(v, box.x + box.width / 2, span);
    push(v, box.x + box.width, span);
    const across = [box.x, box.x + box.width];
    push(h, box.y, across);
    push(h, box.y + box.height / 2, across);
    push(h, box.y + box.height, across);
  }
  for (const p of what.points || []) {
    push(v, p.x, [p.y - 40, p.y + 40]);
    push(h, p.y, [p.x - 40, p.x + 40]);
  }
  return { v, h };
}

function push(list, at, span) {
  if (!isFinite(at)) return;
  const had = list.find((t) => Math.abs(t.at - at) < 0.01);
  if (had) {
    had.span = [Math.min(had.span[0], span[0]), Math.max(had.span[1], span[1])];
    return;
  }
  list.push({ at, span: [span[0], span[1]] });
}

/**
 * How far to nudge a box so it lines up, and what to draw while it does.
 *
 * `invert` is the modifier held: snapping is on by default and the modifier
 * turns it off, which is the way every drawing program does it and the way a
 * person discovers it by accident rather than by reading.
 *
 * @returns {{dx: number, dy: number, guides: {axis: "v"|"h", at: number, span: [number, number]}[]}}
 */
export function snapDelta(box, targets, opts = {}) {
  const out = { dx: 0, dy: 0, guides: [] };
  if (opts.invert) return out;
  const threshold = opts.threshold === undefined ? THRESHOLD : opts.threshold;

  const vertical = [box.x, box.x + box.width / 2, box.x + box.width];
  const horizontal = [box.y, box.y + box.height / 2, box.y + box.height];

  const bestV = nearest(vertical, targets.v, threshold);
  const bestH = nearest(horizontal, targets.h, threshold);
  if (bestV) {
    out.dx = bestV.delta;
    out.guides.push({ axis: "v", at: bestV.target.at, span: grown(bestV.target.span, [box.y, box.y + box.height]) });
  }
  if (bestH) {
    out.dy = bestH.delta;
    out.guides.push({ axis: "h", at: bestH.target.at, span: grown(bestH.target.span, [box.x, box.x + box.width]) });
  }
  if (opts.grid && !bestV) out.dx = Math.round(box.x / opts.grid) * opts.grid - box.x;
  if (opts.grid && !bestH) out.dy = Math.round(box.y / opts.grid) * opts.grid - box.y;
  return out;
}

function nearest(edges, targets, threshold) {
  let best = null;
  for (const edge of edges) {
    for (const target of targets || []) {
      const delta = target.at - edge;
      if (Math.abs(delta) > threshold) continue;
      if (!best || Math.abs(delta) < Math.abs(best.delta)) best = { delta, target };
    }
  }
  return best;
}

function grown(span, other) {
  return [Math.min(span[0], other[0]), Math.max(span[1], other[1])];
}

/**
 * The points a plugin's `snap.points` names, resolved against one item.
 * A point is normalised within the item's own box, because a plugin knows its
 * anchor as a fraction and knows nothing about the canvas.
 */
export function pointsOf(item, box, snap) {
  const names = (snap && snap.points) || [];
  const out = [];
  for (const path of names) {
    const value = read(item, path);
    if (!value || typeof value !== "object") continue;
    const x = Number(value.x);
    const y = Number(value.y);
    if (!isFinite(x) || !isFinite(y)) continue;
    out.push({ x: box.x + x * box.width, y: box.y + y * box.height });
  }
  return out;
}

function read(object, path) {
  let at = object;
  for (const part of String(path || "").split(".").filter(Boolean)) {
    if (!at || typeof at !== "object") return undefined;
    at = at[part];
  }
  return at;
}
