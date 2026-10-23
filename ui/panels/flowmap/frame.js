// One animation frame: the links layer, the dots, the cards layer, then a
// ring for anything that changed state. The layers are only laid down; the
// work is the dots, which are moved in the map's units and drawn in the
// screen's, one path per colour. The frame's cost is kept in `map.stats`
// and on `window.__gmxFlow` for a test to read.

import { along } from "./geom.js";
import { step } from "./dots.js";
import { toneColour } from "./paint.js";

const PULSE_MS = 1500;

export function frame(map, t, still) {
  const t0 = performance.now();
  const dt = still ? 0 : Math.min(0.1, (t - map.last) / 1000);
  map.last = t;
  map.paint();
  const g = map.ctx, v = map.v, col = map.col;
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.clearRect(0, 0, map.canvas.width, map.canvas.height);
  g.drawImage(map.under, 0, 0);
  g.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  const k = map.ease(t0);
  const paths = new Map();
  const zoom = Math.max(0.55, Math.min(1.5, v.s));
  const look = map.look;
  let dots = 0;
  for (const l of map.links) {
    const rate = l.k0 + (l.kbps - l.k0) * k;
    if (!still) step(l, rate, dt, map.budget, t / 1000);
    if (!l.parts.length) continue;
    const dim = look.dims(l.a) || look.dims(l.b) || (look.lit && !look.lit.has(l.id));
    const b = l.box;
    if (b[2] * v.s + v.tx < -10 || b[0] * v.s + v.tx > v.w + 10 || b[3] * v.s + v.ty < -10 || b[1] * v.s + v.ty > v.h + 10) continue;
    const key = `${l.onair ? "live" : l.tone}${dim ? "-dim" : ""}`;
    if (!paths.has(key)) paths.set(key, new Path2D());
    const p = paths.get(key);
    for (const d of l.parts) {
      const [x, y] = along(l, d.t);
      const sx = x * v.s + v.tx, sy = y * v.s + v.ty, r = d.size * zoom;
      p.moveTo(sx + r, sy);
      p.arc(sx, sy, r, 0, Math.PI * 2);
      dots += 1;
    }
  }
  for (const [key, p] of paths) {
    const [tone, dim] = key.split("-");
    g.globalAlpha = dim ? 0.22 : 0.95;
    g.fillStyle = tone === "live" ? col.live : tone === "ok" ? col.accent : toneColour(col, tone);
    g.fill(p);
  }
  g.globalAlpha = 1;
  g.setTransform(1, 0, 0, 1, 0, 0);
  g.drawImage(map.over, 0, 0);
  g.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  rings(map, g, t0);
  if (!still) count(map, t, performance.now() - t0, dots);
}

/** A ring that grows and fades once round a node that changed state. */
function rings(map, g, now) {
  if (!map.pulses.length) return;
  map.pulses = map.pulses.filter((p) => now - p.t0 < PULSE_MS);
  const v = map.v;
  for (const p of map.pulses) {
    const n = map.byId.get(p.id);
    if (!n || n.hidden) continue;
    const q = (now - p.t0) / PULSE_MS;
    const grow = 4 + 26 * q;
    g.beginPath();
    g.roundRect((n.x - n.w / 2) * v.s + v.tx - grow, (n.y - n.h / 2) * v.s + v.ty - grow, n.w * v.s + grow * 2, n.h * v.s + grow * 2, 8 + grow);
    g.strokeStyle = toneColour(map.col, p.tone);
    g.globalAlpha = (1 - q) * 0.85;
    g.lineWidth = 3 * (1 - q) + 0.5;
    g.stroke();
  }
  g.globalAlpha = 1;
}

function count(map, t, work, dots) {
  const s = map.stats;
  s.frames += 1;
  s.work += work;
  s.maxWork = Math.max(s.maxWork, work);
  if (!s.t0) s.t0 = t;
  if (t - s.t0 < 1000) return;
  s.fps = (s.frames * 1000) / (t - s.t0);
  s.avgWork = s.work / s.frames;
  s.dots = dots;
  window.__gmxFlow = { fps: s.fps, avgWork: s.avgWork, maxWork: s.maxWork, dots, nodes: map.nodes.length, links: map.links.length };
  Object.assign(s, { frames: 0, work: 0, maxWork: 0, t0: t });
}
