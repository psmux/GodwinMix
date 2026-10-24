// What lights up and what dims. Hovering a node lights its whole path:
// everything upstream of it back to where it comes in, and everything
// downstream to where it goes out. A filter keeps the shows that match and
// the paths through them, and dims the rest.

/** The nodes and links on every path through `id`, ids in one set. */
export function pathOf(g, id) {
  const lit = new Set();
  if (!id) return null;
  const link = g.links.get(id);
  const walk = (start, side, uses) => {
    const todo = [start];
    while (todo.length) {
      const at = todo.pop();
      if (lit.has(at) && at !== start) continue;
      lit.add(at);
      for (const l of (side === "up" ? g.into : g.from).get(at) || []) {
        if (l.uses && !uses) continue;
        lit.add(l.id);
        const next = side === "up" ? l.a : l.b;
        if (!lit.has(next)) todo.push(next);
      }
    }
  };
  if (link) {
    lit.add(link.id);
    walk(link.a, "up");
    walk(link.b, "down");
    return lit;
  }
  const n = g.nodes.get(id);
  if (!n) return null;
  if (n.kind === "gpu") {
    for (const l of g.from.get(id) || []) {
      lit.add(l.id);
      walk(l.b, "up");
      walk(l.b, "down");
    }
    lit.add(id);
    return lit;
  }
  walk(id, "up");
  walk(id, "down");
  for (const at of [...lit]) for (const l of g.into.get(at) || []) if (l.uses) lit.add(l.id), lit.add(l.a);
  return lit;
}

const hay = (n) => [n.title, n.num, ...(n.lines || []), n.uri, n.group, n.output && (n.output.uri_host || n.output.uri), n.source && n.source.uri].filter(Boolean).join(" ").toLowerCase();

/**
 * The ids a filter keeps, or null when nothing is filtered. `o` is
 * {text, problems, group, family}.
 */
export function kept(g, o) {
  const words = String(o.text || "").toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length && !o.problems && !o.group && !o.family) return null;
  const keep = new Set();
  for (const show of g.nodes.values()) {
    if (show.kind !== "show") continue;
    const path = pathOf(g, show.id);
    const on = [...path].map((id) => g.nodes.get(id)).filter(Boolean);
    if (o.group && show.group !== o.group) continue;
    if (o.family && !on.some((n) => n.family === o.family)) continue;
    if (o.problems && on.every((n) => n.tone === "ok" || n.kind === "gpu")) continue;
    const text = on.map(hay).join(" ");
    if (!words.every((w) => text.includes(w))) continue;
    for (const id of path) keep.add(id);
  }
  return keep;
}
