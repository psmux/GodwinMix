// The shape of each link: a curve from the right edge of one card to the
// left edge of the next, sampled into points once per snapshot so a frame
// only reads them. The GPU's links bow out beside its column.

const STEPS = 20;

function bez(c, t) {
  const u = 1 - t;
  const a = u * u * u, b = 3 * u * u * t, d = 3 * u * t * t, e = t * t * t;
  return { x: a * c[0].x + b * c[1].x + d * c[2].x + e * c[3].x, y: a * c[0].y + b * c[1].y + d * c[2].y + e * c[3].y };
}

export function shape(l, a, b) {
  let c;
  if (l.uses) {
    // Out of the GPU's right side and into the conversion's, bowed into
    // the gap beside the column so it crosses no card.
    const p = { x: a.x + a.w / 2, y: a.y }, q = { x: b.x + b.w / 2, y: b.y };
    const bow = Math.min(44, 12 + Math.abs(q.y - p.y) * 0.04);
    c = [p, { x: p.x + bow, y: p.y }, { x: q.x + bow, y: q.y }, q];
  } else {
    const p = { x: a.x + a.w / 2, y: a.y }, q = { x: b.x - b.w / 2, y: b.y };
    const dx = (q.x - p.x) * 0.5;
    c = [p, { x: p.x + dx, y: p.y }, { x: q.x - dx, y: q.y }, q];
  }
  const pts = [];
  let len = 0;
  for (let i = 0; i <= STEPS; i++) {
    const pt = bez(c, i / STEPS);
    if (i) len += Math.hypot(pt.x - pts[i - 1].x, pt.y - pts[i - 1].y);
    pts.push(pt);
  }
  const xs = c.map((p) => p.x), ys = c.map((p) => p.y);
  return { c, pts, len, box: [Math.min(...xs), Math.min(...ys), Math.max(...xs), Math.max(...ys)] };
}

/** A point `t` of the way along, read off the samples. */
export function along(l, t) {
  const f = t * STEPS, i = Math.min(STEPS - 1, Math.floor(f)), r = f - i;
  const A = l.pts[i], B = l.pts[i + 1];
  return [A.x + (B.x - A.x) * r, A.y + (B.y - A.y) * r];
}

/** How far a point is from the link, in map units. */
export function distance(l, x, y) {
  let best = Infinity;
  for (let i = 0; i < l.pts.length - 1; i++) {
    const A = l.pts[i], B = l.pts[i + 1];
    const dx = B.x - A.x, dy = B.y - A.y;
    const t = Math.max(0, Math.min(1, ((x - A.x) * dx + (y - A.y) * dy) / (dx * dx + dy * dy || 1)));
    best = Math.min(best, Math.hypot(A.x + dx * t - x, A.y + dy * t - y));
  }
  return best;
}
