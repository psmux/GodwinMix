// Handles as data (11 section 5).
//
// A plugin says what handles its item type has and what each one drives:
//
//   gizmos = [
//     { kind = "corner", anchor = [-0.5, -0.5], action = "scale", target = "transform.scale" },
//     { kind = "edge",   anchor = [0.5, 0],     action = "crop",  target = "crop.right" },
//     { kind = "rotate", anchor = [0, -0.6],    action = "rotate", target = "transform.rotation" },
//     { kind = "dial",   anchor = [0.4, 0.4],   action = "set",   target = "params.key_tolerance" }
//   ]
//
// Nothing in that is code, which is the point: the Tkinter designer draws the
// same handles as the web composer from the same manifest, and both send back a
// named command rather than a coordinate. This file turns the data into
// positions, and turns a drag on one of them into the props to assign.
//
// The anchor is normalised about the item's centre: (-0.5, -0.5) is the top
// left corner of the box, (0, -0.6) is above the top edge, (0.5, 0.5) the
// bottom right. It survives a canvas change and a resize, which pixels do not.

import { itemRect } from "./geometry.js";

/** What an item gets when its plugin says nothing: move, scale, rotate. */
export const DEFAULT_GIZMOS = [
  { kind: "cage", anchor: [0, 0], action: "move", target: "transform.position" },
  { kind: "corner", anchor: [-0.5, -0.5], action: "scale", target: "transform.frame" },
  { kind: "corner", anchor: [0.5, -0.5], action: "scale", target: "transform.frame" },
  { kind: "corner", anchor: [-0.5, 0.5], action: "scale", target: "transform.frame" },
  { kind: "corner", anchor: [0.5, 0.5], action: "scale", target: "transform.frame" },
  { kind: "edge", anchor: [0, -0.5], action: "scale", target: "transform.frame" },
  { kind: "edge", anchor: [0, 0.5], action: "scale", target: "transform.frame" },
  { kind: "edge", anchor: [-0.5, 0], action: "scale", target: "transform.frame" },
  { kind: "edge", anchor: [0.5, 0], action: "scale", target: "transform.frame" },
  { kind: "rotate", anchor: [0, -0.62], action: "rotate", target: "transform.rotation" },
];

/** The cursor each direction wants, so a corner says what it will do. */
const CURSORS = {
  "-1,-1": "nwse-resize",
  "1,1": "nwse-resize",
  "1,-1": "nesw-resize",
  "-1,1": "nesw-resize",
  "0,-1": "ns-resize",
  "0,1": "ns-resize",
  "-1,0": "ew-resize",
  "1,0": "ew-resize",
};

/**
 * A named archetype, for the short spelling: `gizmos = ["corner", "rotate"]`.
 *
 * The manifest takes a list of names, which is what most plugins want to say.
 * The long spelling from 11 section 5, a table per handle with its own anchor
 * and target, is accepted in the same list, so a plugin with a dial on one of
 * its own parameters writes that one out and leaves the rest as names.
 */
export const ARCHETYPES = {
  cage: [DEFAULT_GIZMOS[0]],
  move: [DEFAULT_GIZMOS[0]],
  corner: DEFAULT_GIZMOS.slice(1, 5),
  edge: DEFAULT_GIZMOS.slice(5, 9),
  scale: DEFAULT_GIZMOS.slice(1, 9),
  rotate: [DEFAULT_GIZMOS[9]],
  crop: [
    { kind: "edge", anchor: [-0.5, 0], action: "crop", target: "crop.left" },
    { kind: "edge", anchor: [0.5, 0], action: "crop", target: "crop.right" },
    { kind: "edge", anchor: [0, -0.5], action: "crop", target: "crop.top" },
    { kind: "edge", anchor: [0, 0.5], action: "crop", target: "crop.bottom" },
  ],
};

/**
 * The gizmos for one item: the plugin's if it declared any, the defaults if
 * not. Never empty, because an item with no handles cannot be edited at all,
 * which is the one outcome 11 section 5 rules out.
 */
export function gizmosFor(designer) {
  const declared = [];
  for (const entry of (designer && designer.gizmos) || []) {
    if (typeof entry === "string") declared.push(...(ARCHETYPES[entry] || []));
    else if (valid(entry)) declared.push(entry);
  }
  if (!declared.length) return DEFAULT_GIZMOS;
  // A plugin that declares handles still gets to be moved: without a cage
  // nothing could pick the item up, and no plugin should have to remember it.
  return declared.some((g) => g.kind === "cage") ? declared : [DEFAULT_GIZMOS[0], ...declared];
}

function valid(g) {
  return !!g && typeof g.kind === "string" && Array.isArray(g.anchor) && g.anchor.length === 2;
}

/**
 * Where each handle sits, in canvas pixels, for one item's box.
 *
 * @param {{x,y,width,height}} box the derived geometry the core answered with
 * @param {object[]} gizmos
 * @returns {{i, kind, action, target, x, y, dir: [number, number], cursor}[]}
 */
export function handlesFor(box, gizmos) {
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  return (gizmos || []).map((g, i) => {
    const [nx, ny] = g.anchor;
    const dir = [Math.sign(nx), Math.sign(ny)];
    return {
      i,
      kind: g.kind,
      action: g.action || "move",
      target: g.target || "transform.position",
      x: cx + nx * box.width,
      y: cy + ny * box.height,
      dir,
      cursor: g.cursor || CURSORS[`${dir[0]},${dir[1]}`] || (g.kind === "rotate" ? "grab" : "move"),
    };
  });
}

/** The handle under a point, nearest first, or null. `r` is in canvas pixels. */
export function hitTest(handles, x, y, r) {
  let best = null;
  let bestD = r * r;
  for (const h of handles) {
    if (h.kind === "cage") continue;
    const d = (h.x - x) * (h.x - x) + (h.y - y) * (h.y - y);
    if (d <= bestD) {
      bestD = d;
      best = h;
    }
  }
  return best;
}

/**
 * A drag on one handle, as the props to assign.
 *
 * Pure arithmetic: it is handed where the item is and where the pointer went,
 * and it answers with the `props` for `scene.item.set` and the box to draw
 * while the core catches up. No DOM, no client, no state, so the Python and
 * TypeScript ports are the same function and the fixtures check all three.
 *
 * @param {object} handle     from handlesFor
 * @param {object} start      {box, transform} as they were when the drag began
 * @param {number} dx         canvas pixels moved since then
 * @param {number} dy
 * @param {{aspect?: boolean, centre?: boolean, step?: number, min?: number, max?: number}} mods
 * @returns {{props: object, box: object}}
 */
export function applyDrag(handle, start, dx, dy, mods = {}) {
  const box = start.box;
  const t = start.transform || {};
  switch (handle.action) {
    case "move":
      return moved(box, t, dx, dy);
    case "scale":
      return scaled(handle, box, t, dx, dy, mods);
    case "crop":
      return cropped(handle, box, t, dx, dy);
    case "rotate":
      return rotated(handle, box, t, mods);
    case "set":
      return dialled(handle, t, dx, mods);
    default:
      return moved(box, t, dx, dy);
  }
}

function moved(box, t, dx, dy) {
  const position = { x: num(t.position && t.position.x) + dx, y: num(t.position && t.position.y) + dy };
  return {
    props: { transform: { position } },
    box: Object.assign({}, box, { x: box.x + dx, y: box.y + dy }),
  };
}

/**
 * A corner or an edge. The opposite side stays where it is, which is what
 * every drawing program does and what a person expects when they pull a corner.
 */
function scaled(handle, box, t, dx, dy, mods) {
  const [sx, sy] = handle.dir;
  let w = box.width + sx * dx * (mods.centre ? 2 : 1);
  let h = box.height + sy * dy * (mods.centre ? 2 : 1);
  if (mods.aspect && box.width > 0 && box.height > 0 && sx !== 0 && sy !== 0) {
    // Shift keeps the shape: the larger change wins so the box follows the hand.
    const ratio = box.width / box.height;
    if (Math.abs(w / ratio - box.height) > Math.abs(h - box.height)) h = w / ratio;
    else w = h * ratio;
  }
  w = Math.max(16, w);
  h = Math.max(16, h);
  let x = box.x;
  let y = box.y;
  if (mods.centre) {
    x = box.x + (box.width - w) / 2;
    y = box.y + (box.height - h) / 2;
  } else {
    if (sx < 0) x = box.x + (box.width - w);
    if (sy < 0) y = box.y + (box.height - h);
  }
  const scale = t.scale || { x: 1, y: 1 };
  const anchor = t.anchor || { x: 0, y: 0 };
  const frame = { w: w / (num(scale.x, 1) || 1), h: h / (num(scale.y, 1) || 1) };
  const position = { x: x + num(anchor.x) * w, y: y + num(anchor.y) * h };
  return { props: { transform: { frame, position } }, box: Object.assign({}, box, { x, y, width: w, height: h }) };
}

/**
 * Crop is normalised 0 to 1 of the content's own pixels, not canvas pixels, so
 * it survives a canvas change (vMix and CasparCG do this; OBS crops in pixels
 * and loses them at 720p).
 */
function cropped(handle, box, t, dx, dy) {
  const side = String(handle.target || "").split(".").pop();
  const crop = Object.assign({ left: 0, top: 0, right: 0, bottom: 0 }, t.crop || {});
  const along = side === "left" || side === "right" ? dx / Math.max(1, box.width) : dy / Math.max(1, box.height);
  const sign = side === "right" || side === "bottom" ? -1 : 1;
  const other = { left: "right", right: "left", top: "bottom", bottom: "top" }[side];
  crop[side] = clamp(num(crop[side]) + along * sign, 0, 0.95 - num(crop[other]));
  return { props: { crop }, box };
}

/**
 * Degrees clockwise about the item's centre, snapped to 15 with Shift.
 * The angle is read from where the pointer is, not from how far it travelled:
 * a rotation handle that integrates deltas drifts over a long drag.
 */
function rotated(handle, box, t, mods) {
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const p = mods.pointer || { x: handle.x, y: handle.y };
  const at = Math.atan2(p.y - cy, p.x - cx) * (180 / Math.PI) + 90;
  let deg = mods.aspect ? Math.round(at / 15) * 15 : at;
  deg = ((deg % 360) + 360) % 360;
  return { props: { transform: { rotation: round(deg, 2) } }, box };
}

/**
 * A dial: sideways travel is the number. The plugin's schema says what the
 * ends are; without one, 0 to 1 over 200 pixels, which is what a person
 * expects of a knob they have never seen.
 */
function dialled(handle, t, dx, mods) {
  const min = mods.min === undefined ? 0 : mods.min;
  const max = mods.max === undefined ? 1 : mods.max;
  const from = mods.from === undefined ? min : mods.from;
  const span = (max - min) / 200;
  let value = clamp(from + dx * span, min, max);
  if (mods.step) value = Math.round(value / mods.step) * mods.step;
  return { props: pathProps(handle.target, round(value, 4)), box: null };
}

/** `params.key_tolerance` becomes `{params: {key_tolerance: value}}`. */
export function pathProps(path, value) {
  const parts = String(path || "").split(".").filter(Boolean);
  if (!parts.length) return {};
  const out = {};
  let at = out;
  for (let i = 0; i < parts.length - 1; i += 1) {
    at[parts[i]] = {};
    at = at[parts[i]];
  }
  at[parts[parts.length - 1]] = value;
  return out;
}

/** Read `transform.frame.w` out of an object, or undefined. */
export function pathValue(object, path) {
  let at = object;
  for (const part of String(path || "").split(".").filter(Boolean)) {
    if (at === null || at === undefined || typeof at !== "object") return undefined;
    at = at[part];
  }
  return at;
}

function num(v, fallback) {
  return typeof v === "number" && isFinite(v) ? v : fallback === undefined ? 0 : fallback;
}

function clamp(v, lo, hi) {
  return Math.min(hi, Math.max(lo, v));
}

function round(v, places) {
  const f = Math.pow(10, places);
  return Math.round(v * f) / f;
}

export { itemRect };
