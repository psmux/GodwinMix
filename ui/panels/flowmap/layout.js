// Where everything sits, in the map's own coordinates. Four columns left to
// right, a band of rows per show, the channels that share an input address
// kept together under one tall input card that spans them. Deterministic:
// the same graph always lands in the same places. On a phone the processing
// column folds into the links, so three columns fit across.

const WIDE = { w: [212, 196, 188, 222], gap: 96, row: 34, card: 28, mixRow: 56, mixCard: 46, between: 16, top: 54 };
const NARROW = { w: [118, 124, 0, 118], gap: 26, row: 36, card: 30, mixRow: 44, mixCard: 38, between: 12, top: 40 };

export function columns(narrow) {
  const k = narrow ? NARROW : WIDE;
  const x = [];
  let at = 0;
  for (const w of k.w) {
    x.push(at + w / 2);
    if (w) at += w + k.gap;
  }
  return { ...k, x, width: at - k.gap };
}

/** On a phone, the processing column becomes the links through it. */
function fold(g) {
  const mid = [...g.nodes.values()].filter((n) => n.col === 2);
  for (const p of mid) {
    const into = [...g.links.values()].filter((l) => l.b === p.id && !l.uses);
    const out = [...g.links.values()].filter((l) => l.a === p.id);
    for (const l of out) {
      for (const i of into) g.links.set(`${i.a}>${l.b}`, { ...l, id: `${i.a}>${l.b}`, a: i.a });
      g.links.delete(l.id);
    }
    for (const l of [...g.links.values()]) if (l.b === p.id || l.a === p.id) g.links.delete(l.id);
    const show = g.nodes.get(`show:${p.show}`);
    if (show && p.gpu) show.badge = "GPU";
    g.nodes.delete(p.id);
  }
  g.index();
}

const byNumber = (a, b) => (Number(a.num) || 1e9) - (Number(b.num) || 1e9) || String(a.title).localeCompare(String(b.title));

/** The shows in the order their bands run: mixed shows first, then channels by input. */
function order(g) {
  const shows = [...g.nodes.values()].filter((n) => n.kind === "show");
  const mixed = shows.filter((n) => n.mixed);
  const direct = shows.filter((n) => !n.mixed);
  const size = new Map();
  for (const n of direct) size.set(n.group, (size.get(n.group) || 0) + 1);
  const shared = (n) => (size.get(n.group) || 0) > 1;
  direct.sort((a, b) => Number(shared(b)) - Number(shared(a)) || (shared(a) ? String(a.group).localeCompare(String(b.group), undefined, { numeric: true }) : 0) || byNumber(a, b));
  return [...mixed, ...direct];
}

const kids = (g, id, col) => (g.from.get(id) || []).filter((l) => !l.uses).map((l) => g.nodes.get(l.b)).filter((n) => n && n.col === col);

export function layout(g, narrow) {
  if (narrow) fold(g);
  const k = columns(narrow);
  const put = (n, y, h) => Object.assign(n, { x: k.x[n.col], y, w: k.w[n.col], h });
  const gpu = [...g.nodes.values()].filter((n) => n.kind === "gpu");
  gpu.forEach((n, i) => put(n, k.top + 30 + i * 70, 60));
  let y = k.top + gpu.length * 70;
  let last = null;
  for (const show of order(g)) {
    if (last !== null && (show.mixed || last.mixed || show.group !== last.group)) y += k.between;
    last = show;
    const row = show.mixed ? k.mixRow : k.row;
    const card = show.mixed ? k.mixCard : k.card;
    const into = (g.into.get(show.id) || []).map((l) => g.nodes.get(l.a)).filter((n) => n && n.kind === "source");
    const mids = kids(g, show.id, 2);
    const outs = [];
    for (const m of mids) for (const o of kids(g, m.id, 3).concat(...kids(g, m.id, 2).map((r) => kids(g, r.id, 3)))) if (!outs.includes(o)) outs.push(o);
    for (const o of kids(g, show.id, 3)) if (!outs.includes(o)) outs.push(o);
    const procs = mids.concat(...mids.map((m) => kids(g, m.id, 2)));
    const rows = Math.max(1, into.length, outs.length, procs.length);
    const top = y;
    into.forEach((n, i) => put(n, top + row * (i + 0.5), card));
    outs.forEach((n, i) => put(n, top + row * (i + 0.5), card));
    put(show, top + (row * rows) / 2, Math.min(row * rows - 6, show.mixed ? Math.max(card, 56) : card));
    procs.forEach((p, i) => {
      const served = kids(g, p.id, 3);
      const mean = served.length ? served.reduce((s, o) => s + o.y, 0) / served.length : top + row * (i + 0.5);
      put(p, p.kind === "program" ? top + row / 2 : Math.max(mean, top + row * (i + 0.5)), card);
    });
    y = top + row * rows;
  }
  spanInputs(g, k);
  for (const n of g.nodes.values()) if (n.x == null) Object.assign(n, { x: -1e5, y: -1e5, w: 0, h: 0, hidden: true });
  return { width: k.width, height: y + k.top, cols: k };
}

/** An input card runs from its first programme's row to its last. */
function spanInputs(g, k) {
  for (const n of g.nodes.values()) {
    if (n.kind !== "input") continue;
    const ys = (g.from.get(n.id) || []).map((l) => g.nodes.get(l.b)).filter((m) => m && m.y != null).map((m) => m.y);
    if (!ys.length) continue;
    const lo = Math.min(...ys) - k.card / 2, hi = Math.max(...ys) + k.card / 2;
    Object.assign(n, { x: k.x[0], y: (lo + hi) / 2, w: k.w[0], h: Math.max(k.card, hi - lo) });
  }
}
