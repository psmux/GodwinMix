// The moving dots. A link's dots come at a rate and move at a speed set by
// its bit rate, on a log scale so a 128 kbit/s sound feed and a 60 Mbit/s
// multicast group are both visible and different. Rates are eased from one
// snapshot to the next. A link with errors stutters: its dots stop and
// start and some never arrive. All dots of one colour are one path, filled
// once per frame.

const PER_LINK = 26;

/** Dots a second, speed in map units a second, and radius, for a rate. */
export function dyn(kbps) {
  if (!kbps || kbps < 20) return { rate: 0, speed: 0, size: 0 };
  const L = Math.log10(kbps / 100 + 1);
  return { rate: Math.min(7, 0.4 + 1.6 * L), speed: 46 + 24 * L, size: 1.6 + 0.45 * L };
}

/** Move one link's dots on by `dt` seconds; `v` is its eased rate, `budget` 0 to 1. */
export function step(l, v, dt, budget, t) {
  if (!v || l.uses) {
    l.parts.length = 0;
    return;
  }
  const d = dyn(v);
  const lenF = Math.max(0.4, Math.min(1.6, l.len / 240));
  l.acc += d.rate * lenF * dt * budget;
  while (l.acc >= 1) {
    l.acc -= 1;
    if (l.parts.length < PER_LINK) l.parts.push({ t: 0, speed: d.speed * (0.85 + Math.random() * 0.3), size: d.size, ph: Math.random() * 6.28 });
  }
  let w = 0;
  for (const p of l.parts) {
    let f = 1;
    if (l.err) {
      f = Math.sin(t * 6 + p.ph) > 1 - 2 * l.err ? 0.04 : 1.5;
      if (Math.random() < l.err * dt * 0.6) continue;
    }
    p.t += (p.speed * f * dt) / Math.max(20, l.len);
    if (p.t < 1) l.parts[w++] = p;
  }
  l.parts.length = w;
}

/** How many dots the links would hold at their rates, to keep the total in hand. */
export function expected(links) {
  let n = 0;
  for (const l of links) {
    const d = dyn(l.kbps);
    if (d.rate) n += Math.min(PER_LINK, d.rate * Math.max(0.4, Math.min(1.6, l.len / 240)) * (l.len / d.speed));
  }
  return n;
}
