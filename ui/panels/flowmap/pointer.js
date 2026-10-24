// The pointer on the map: hover finds a node or a link and says so, a drag
// moves the map, the wheel scrolls it, Ctrl and the wheel or a pinch zoom
// it about the pointer, and a press that did not move picks what is under
// it. Touch and mouse alike.

import { distance } from "./geom.js";

/** The node or link at a point on screen, cards first. */
export function hitAt(map, sx, sy) {
  const [x, y] = map.world(sx, sy);
  for (let i = map.nodes.length - 1; i >= 0; i--) {
    const n = map.nodes[i];
    if (!n.hidden && Math.abs(n.x - x) <= n.w / 2 && Math.abs(n.y - y) <= n.h / 2) return { kind: "node", id: n.id };
  }
  const near = 6 / map.v.s;
  let best = null, bd = near;
  for (const l of map.links) {
    if (l.uses && !(map.look.lit && map.look.lit.has(l.id))) continue;
    if (x < l.box[0] - near || x > l.box[2] + near || y < l.box[1] - near || y > l.box[3] + near) continue;
    const d = distance(l, x, y);
    if (d < bd) [bd, best] = [d, l];
  }
  return best ? { kind: "link", id: best.id } : null;
}

export function pointer(canvas, map, on) {
  const down = new Map();
  let moved = 0, pinch = 0;
  const at = (e) => {
    const r = canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    down.set(e.pointerId, at(e));
    moved = 0;
    if (down.size === 2) {
      const [a, b] = [...down.values()];
      pinch = Math.hypot(a[0] - b[0], a[1] - b[1]);
    }
  });
  canvas.addEventListener("pointermove", (e) => {
    const p = at(e);
    if (!down.has(e.pointerId)) return on.hover(hitAt(map, ...p), p);
    const was = down.get(e.pointerId);
    down.set(e.pointerId, p);
    if (down.size === 2) {
      const [a, b] = [...down.values()];
      const d = Math.hypot(a[0] - b[0], a[1] - b[1]);
      if (pinch) map.zoomAt((a[0] + b[0]) / 2, (a[1] + b[1]) / 2, d / pinch);
      pinch = d;
      moved += 10;
    } else {
      moved += Math.abs(p[0] - was[0]) + Math.abs(p[1] - was[1]);
      if (moved > 4) map.pan(p[0] - was[0], p[1] - was[1]), on.hover(null, p);
    }
  });
  const up = (e) => {
    const p = down.get(e.pointerId);
    down.delete(e.pointerId);
    if (down.size < 2) pinch = 0;
    if (p && moved <= 4 && e.type === "pointerup") on.pick(hitAt(map, ...p), p);
  };
  canvas.addEventListener("pointerup", up);
  canvas.addEventListener("pointercancel", up);
  canvas.addEventListener("pointerleave", (e) => { if (!down.size && e.pointerType === "mouse") on.hover(null); });
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    const [x, y] = at(e);
    // Ctrl and the wheel, or a pinch on a trackpad, zooms; the wheel alone scrolls.
    if (e.ctrlKey || e.metaKey) map.zoomAt(x, y, Math.exp(-e.deltaY * 0.01));
    else map.pan(-(e.shiftKey ? e.deltaY : e.deltaX), e.shiftKey ? 0 : -e.deltaY);
    on.hover(null);
  }, { passive: false });
  canvas.addEventListener("dblclick", (e) => map.zoomAt(...at(e), 1.6));
}
