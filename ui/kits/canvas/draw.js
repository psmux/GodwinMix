// The canvas kit's painter, for a browser.
//
// Everything above this file is arithmetic that the Python and Rust kits share
// (11 section 6). This file is the one part that knows about a 2D context, and
// it is deliberately thin: a Tkinter port draws the same shapes from the same
// numbers with `create_rectangle`, and a Godot port with its own gizmos.
//
// It draws over whatever the preview transport put behind it: WHEP, MJPEG, a
// still, or nothing at all. It never draws the picture itself.

import { safeAreas, ticks } from "./safe.js";

const HANDLE = 8;

/** Colours read off the theme, so high contrast gets high contrast handles. */
function palette(root) {
  const style = getComputedStyle(root || document.documentElement);
  const read = (name, fallback) => (style.getPropertyValue(name) || "").trim() || fallback;
  return {
    select: read("--select", "#3b82f6"),
    live: read("--live", "#ff2a36"),
    guide: read("--accent", "#3b82f6"),
    dim: read("--dim", "#9aa0a6"),
    panel: read("--panel", "#1b1d1f"),
  };
}

/**
 * One pass over the whole overlay.
 *
 * @param {CanvasRenderingContext2D} ctx
 * @param {{view, boxes, selected, handles, guides, marquee, safe, rulers, root}} scene
 */
export function paint(ctx, scene) {
  const view = scene.view;
  const c = palette(scene.root);
  ctx.save();
  ctx.clearRect(0, 0, ctx.canvas.width, ctx.canvas.height);
  ctx.lineJoin = "round";

  if (scene.safe) drawSafe(ctx, view, c);
  if (scene.rulers) drawRulers(ctx, view, c);

  for (const entry of scene.boxes || []) {
    const box = view.boxToSurface(entry.box);
    const on = scene.selected && scene.selected.has(entry.id);
    ctx.lineWidth = on ? 2 : 1;
    ctx.strokeStyle = on ? c.select : c.dim;
    ctx.globalAlpha = on ? 1 : 0.55;
    ctx.strokeRect(box.x + 0.5, box.y + 0.5, box.width - 1, box.height - 1);
    ctx.globalAlpha = 1;
    if (entry.label && on) label(ctx, box, entry.label, c);
  }

  for (const guide of scene.guides || []) drawGuide(ctx, view, guide, c);
  for (const handle of scene.handles || []) drawHandle(ctx, view, handle, c);

  if (scene.marquee) {
    const m = view.boxToSurface(scene.marquee);
    ctx.fillStyle = c.select;
    ctx.globalAlpha = 0.14;
    ctx.fillRect(m.x, m.y, m.width, m.height);
    ctx.globalAlpha = 1;
    ctx.strokeStyle = c.select;
    ctx.setLineDash([4, 3]);
    ctx.lineWidth = 1;
    ctx.strokeRect(m.x + 0.5, m.y + 0.5, m.width, m.height);
    ctx.setLineDash([]);
  }
  ctx.restore();
}

function drawSafe(ctx, view, c) {
  const areas = safeAreas(view.canvas);
  ctx.setLineDash([6, 5]);
  ctx.lineWidth = 1;
  for (const [name, box] of [["action", areas.action], ["title", areas.title]]) {
    const r = view.boxToSurface(box);
    ctx.strokeStyle = c.dim;
    ctx.globalAlpha = name === "title" ? 0.75 : 0.45;
    ctx.strokeRect(r.x + 0.5, r.y + 0.5, r.width, r.height);
  }
  ctx.globalAlpha = 1;
  ctx.setLineDash([]);
}

function drawRulers(ctx, view, c) {
  ctx.fillStyle = c.dim;
  ctx.font = "10px system-ui, sans-serif";
  ctx.globalAlpha = 0.7;
  for (const at of ticks(view.canvas.width, view.scale)) {
    const p = view.toSurface(at, 0);
    ctx.fillRect(p.x, p.y, 1, 6);
    if (at) ctx.fillText(String(at), p.x + 2, p.y + 14);
  }
  for (const at of ticks(view.canvas.height, view.scale)) {
    const p = view.toSurface(0, at);
    ctx.fillRect(p.x, p.y, 6, 1);
  }
  ctx.globalAlpha = 1;
}

function drawGuide(ctx, view, guide, c) {
  ctx.strokeStyle = c.live;
  ctx.lineWidth = 1;
  ctx.beginPath();
  if (guide.axis === "v") {
    const a = view.toSurface(guide.at, guide.span[0]);
    const b = view.toSurface(guide.at, guide.span[1]);
    ctx.moveTo(a.x + 0.5, a.y);
    ctx.lineTo(b.x + 0.5, b.y);
  } else {
    const a = view.toSurface(guide.span[0], guide.at);
    const b = view.toSurface(guide.span[1], guide.at);
    ctx.moveTo(a.x, a.y + 0.5);
    ctx.lineTo(b.x, b.y + 0.5);
  }
  ctx.stroke();
}

function drawHandle(ctx, view, handle, c) {
  const p = view.toSurface(handle.x, handle.y);
  ctx.fillStyle = c.panel;
  ctx.strokeStyle = c.select;
  ctx.lineWidth = 1.5;
  if (handle.kind === "rotate" || handle.kind === "dial") {
    ctx.beginPath();
    ctx.arc(p.x, p.y, HANDLE / 2 + 1, 0, Math.PI * 2);
    ctx.fill();
    ctx.stroke();
    return;
  }
  if (handle.kind === "cage") return;
  ctx.fillRect(p.x - HANDLE / 2, p.y - HANDLE / 2, HANDLE, HANDLE);
  ctx.strokeRect(p.x - HANDLE / 2, p.y - HANDLE / 2, HANDLE, HANDLE);
}

function label(ctx, box, text, c) {
  ctx.font = "11px system-ui, sans-serif";
  const w = ctx.measureText(text).width + 8;
  const y = box.y > 16 ? box.y - 16 : box.y + 2;
  ctx.fillStyle = c.panel;
  ctx.globalAlpha = 0.85;
  ctx.fillRect(box.x, y, w, 15);
  ctx.globalAlpha = 1;
  ctx.fillStyle = c.select;
  ctx.fillText(text, box.x + 4, y + 11);
}

/** The pixel size a handle wants, in canvas units, for hit testing. */
export function grabRadius(view) {
  return view.lengthFromSurface(HANDLE + 4);
}
