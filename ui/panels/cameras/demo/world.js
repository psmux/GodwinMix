// The demo's room: one stage seen by three cameras, with people walking on
// it. Coordinates are fractions of the stage, x from left to right and y
// from top to bottom, on a 16:9 stage, so a camera's view is a square in
// these units. Nothing here draws; paint.js does that.

export const FLOOR = 0.86;

const PEOPLE = [
  { id: "person-1", coat: "#3d6fd8", skin: "#e2b493", hair: "#3a2a1e", h: 0.44, x: 0.5, speed: 0.035 },
  { id: "person-2", coat: "#c2533b", skin: "#8d5a3b", hair: "#1d1612", h: 0.4, x: 0.27, speed: 0.05 },
  { id: "person-3", coat: "#3f9a6b", skin: "#c99a73", hair: "#6b4a2a", h: 0.42, x: 0.76, speed: 0.04 },
];

/** A small seeded random, so a test sees the same walk every time. */
export function rng(seed) {
  let s = seed >>> 0 || 1;
  return () => {
    s = (s * 1664525 + 1013904223) >>> 0;
    return s / 4294967296;
  };
}

export function world(seed = 7) {
  const rand = rng(seed);
  const people = PEOPLE.map((p) => ({ ...p, goal: p.x, wait: rand() * 2, dir: 1 }));
  return { people, rand, t: 0 };
}

/** Move everyone on by `dt` seconds: walk to a spot, stand a while, pick another. */
export function stepWorld(w, dt) {
  w.t += dt;
  for (const p of w.people) {
    if (p.wait > 0) {
      p.wait -= dt;
      if (p.wait <= 0) p.goal = 0.12 + w.rand() * 0.76;
      continue;
    }
    const d = p.goal - p.x;
    const stepBy = Math.sign(d) * Math.min(Math.abs(d), p.speed * dt);
    p.x += stepBy;
    if (stepBy) p.dir = Math.sign(stepBy);
    if (Math.abs(p.goal - p.x) < 0.002) p.wait = 1.5 + w.rand() * 4;
  }
}

/** A person's box on the stage: [x, y, w, h]. */
export function bodyBox(p) {
  const bw = p.h * 0.3;
  return [p.x - bw / 2, FLOOR - p.h, bw, p.h];
}

/** What a camera at view `v` ({cx, cy, w}) sees of the stage, mirrored or not. */
export function viewRect(v) {
  return [v.cx - v.w / 2, v.cy - v.w / 2, v.w, v.w];
}

/** Where a stage x lands for a camera that looks from the other side. */
export const seenX = (cam, x) => (cam.look && cam.look.mirror ? 1 - x : x);

/** The boxes a camera's tracker would report, in fractions of its picture. */
export function observe(w, cam) {
  const [vx, vy, vw, vh] = viewRect(cam.view);
  const out = [];
  for (const p of w.people) {
    const [bx, by, bw, bh] = bodyBox({ ...p, x: seenX(cam, p.x) });
    const box = [(bx - vx) / vw, (by - vy) / vh, bw / vw, bh / vh];
    if (box[0] + box[2] < 0.02 || box[0] > 0.98 || box[1] > 0.98 || box[1] + box[3] < 0.02) continue;
    const seen = Math.min(1, box[0] + box[2]) - Math.max(0, box[0]);
    out.push({ id: p.id, box: box.map((n) => Math.round(n * 1000) / 1000), score: Math.round((0.7 + 0.3 * Math.min(1, seen / box[2])) * 100) / 100 });
  }
  return out;
}

/** The nearest person to a point in a camera's picture, or null. */
export function personAt(w, cam, x, y) {
  let best = null;
  let bestD = 0.12;
  for (const o of observe(w, cam)) {
    const [bx, by, bw, bh] = o.box;
    const inside = x >= bx && x <= bx + bw && y >= by && y <= by + bh;
    const d = inside ? 0 : Math.hypot(x - (bx + bw / 2), y - (by + bh / 2));
    if (d < bestD) (best = o.id), (bestD = d);
  }
  return best;
}
