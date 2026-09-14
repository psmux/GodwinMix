// The canvas kit: geometry, handles, snapping and safe areas.
//
// A typed port of ui/kits/canvas/geometry.js, gizmos.js, snap.js and safe.js.
// The reference modules in ui/kits hold the reasoning, ui/kits/fixtures.json
// holds the behaviour, and test/kits.test.ts replays the second against this
// file so the three kits cannot drift apart.
//
// There is no DOM in here and there is no drawing in here. ui/kits/canvas/
// draw.js paints a 2D context and stays in the reference UI; everything below
// is arithmetic over plain numbers, so a node process, a Deno script and a
// bundle all get the same answers.

// --------------------------------------------------------------- the shapes

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Size {
  width: number;
  height: number;
}

export interface Point {
  x: number;
  y: number;
}

/** An item's transform, as the document holds it. Every part is optional. */
export interface Transform {
  position?: Partial<Point>;
  frame?: { w?: number; h?: number };
  scale?: Partial<Point>;
  anchor?: Partial<Point>;
  rotation?: number;
  crop?: Partial<Crop>;
  [key: string]: unknown;
}

export interface Crop {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** The transform a rectangle asks for: a frame and a position, nothing else. */
export interface RectTransform {
  frame: { w: number; h: number };
  position: Point;
}

// ------------------------------------------------------------- the geometry

/**
 * The box an item occupies, before rotation, from its transform.
 *
 * This is the one piece of scene arithmetic a client is allowed to do for
 * itself. Everything else comes from the core: every mutating command answers
 * with the flattened geometry, so a client never recomputes a layout and never
 * disagrees with the compositor about where an item is. What a client does need
 * locally is the reverse map, because a drag produces a rectangle and the
 * document wants a transform:
 *
 *     x = position.x - anchor.x * (frame.w * scale.x)
 *
 * That is `geometry.rs::item_rect`, written out once here and ported with the
 * fixtures, which is why the handles land on the picture in all three kits.
 */
export function itemRect(transform: Transform | null | undefined, canvas: Size): Rect {
  const t = transform || {};
  const frame = t.frame || { w: canvas.width, h: canvas.height };
  const scale = t.scale || { x: 1, y: 1 };
  const anchor = t.anchor || { x: 0, y: 0 };
  const w = n(frame.w, canvas.width) * n(scale.x, 1);
  const h = n(frame.h, canvas.height) * n(scale.y, 1);
  return {
    x: n(t.position && t.position.x) - n(anchor.x) * w,
    y: n(t.position && t.position.y) - n(anchor.y) * h,
    width: w,
    height: h,
  };
}

/** The transform a rectangle asks for, keeping the item's anchor and scale. */
export function rectToTransform(rect: Rect, transform: Transform | null | undefined): RectTransform {
  const t = transform || {};
  const scale = t.scale || { x: 1, y: 1 };
  const anchor = t.anchor || { x: 0, y: 0 };
  return {
    frame: { w: rect.width / (n(scale.x, 1) || 1), h: rect.height / (n(scale.y, 1) || 1) },
    position: { x: rect.x + n(anchor.x) * rect.width, y: rect.y + n(anchor.y) * rect.height },
  };
}

/** Do two boxes touch? */
export function overlaps(a: Rect, b: Rect): boolean {
  return a.x < b.x + b.width && b.x < a.x + a.width && a.y < b.y + b.height && b.y < a.y + a.height;
}

/** Is a point inside a box? */
export function inside(box: Rect, x: number, y: number): boolean {
  return x >= box.x && x <= box.x + box.width && y >= box.y && y <= box.y + box.height;
}

/** Normalise a sweep from its two corners, so dragging up and left works. */
export function rectFrom(x0: number, y0: number, x1: number, y1: number): Rect {
  return {
    x: Math.min(x0, x1),
    y: Math.min(y0, y1),
    width: Math.abs(x1 - x0),
    height: Math.abs(y1 - y0),
  };
}

/** The smallest box round a set of boxes, or null for none. */
export function unionOf(boxes: Rect[] | null | undefined): Rect | null {
  if (!boxes || !boxes.length) return null;
  let x0 = Infinity;
  let y0 = Infinity;
  let x1 = -Infinity;
  let y1 = -Infinity;
  for (const b of boxes) {
    x0 = Math.min(x0, b.x);
    y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.width);
    y1 = Math.max(y1, b.y + b.height);
  }
  return { x: x0, y: y0, width: x1 - x0, height: y1 - y0 };
}

/**
 * Canvas pixels to the surface the preview is drawn on, and back.
 *
 * The preview picture is whatever size the window gave it; the document is in
 * canvas pixels. One object holds the two, so nothing else in the composer
 * multiplies by a ratio and gets it wrong in one place out of five.
 */
export class View {
  canvas!: Size;
  surface!: Size;
  scale!: number;
  offsetX!: number;
  offsetY!: number;

  constructor(canvas?: Size | null, surface?: Size | null) {
    this.set(canvas, surface);
  }

  set(canvas?: Size | null, surface?: Size | null): this {
    this.canvas = canvas || { width: 1920, height: 1080 };
    this.surface = surface || { width: 1, height: 1 };
    // The picture is letterboxed inside the surface, the way `object-fit:
    // contain` draws it, so the handles sit on the picture and not beside it.
    const scale = Math.min(this.surface.width / this.canvas.width, this.surface.height / this.canvas.height);
    this.scale = isFinite(scale) && scale > 0 ? scale : 1;
    this.offsetX = (this.surface.width - this.canvas.width * this.scale) / 2;
    this.offsetY = (this.surface.height - this.canvas.height * this.scale) / 2;
    return this;
  }

  toSurface(x: number, y: number): Point {
    return { x: this.offsetX + x * this.scale, y: this.offsetY + y * this.scale };
  }

  toCanvas(x: number, y: number): Point {
    return { x: (x - this.offsetX) / this.scale, y: (y - this.offsetY) / this.scale };
  }

  boxToSurface(box: Rect): Rect {
    const p = this.toSurface(box.x, box.y);
    return { x: p.x, y: p.y, width: box.width * this.scale, height: box.height * this.scale };
  }

  /** A length in canvas pixels that draws as `px` on the surface. */
  lengthFromSurface(px: number): number {
    return px / this.scale;
  }
}

function n(v: unknown, fallback?: number): number {
  return typeof v === "number" && isFinite(v) ? v : fallback === undefined ? 0 : fallback;
}

// --------------------------------------------------------------- the gizmos

/** What a drag on a handle does. A plugin may name one of its own. */
export type GizmoAction = "move" | "scale" | "crop" | "rotate" | "set" | (string & {});

/**
 * One handle, as a plugin declares it.
 *
 * The anchor is normalised about the item's centre: (-0.5, -0.5) is the top
 * left corner of the box, (0, -0.6) is above the top edge, (0.5, 0.5) the
 * bottom right. It survives a canvas change and a resize, which pixels do not.
 */
export interface Gizmo {
  kind: string;
  anchor: [number, number];
  action?: GizmoAction;
  /** The property a drag writes, such as `transform.frame` or `crop.right`. */
  target?: string;
  cursor?: string;
}

/** A plugin's `designer` block, the part of it this file reads. */
export interface DesignerBlock {
  gizmos?: Array<string | Gizmo>;
  [key: string]: unknown;
}

/** One handle placed on an item's box, in canvas pixels. */
export interface Handle {
  i: number;
  kind: string;
  action: GizmoAction;
  target: string;
  x: number;
  y: number;
  dir: [number, number];
  cursor: string;
}

/** The modifiers a drag was made with, and the ends a dial runs between. */
export interface DragMods {
  /** Shift: keep the shape when scaling, snap to 15 degrees when rotating. */
  aspect?: boolean;
  /** Alt: scale about the centre rather than the opposite side. */
  centre?: boolean;
  step?: number;
  min?: number;
  max?: number;
  /** Where a dial started, when the caller knows the current value. */
  from?: number;
  /** Where the pointer is now, in canvas pixels. A rotation reads this. */
  pointer?: Point;
}

export interface DragResult {
  /** The props for `scene.item.set`. */
  props: Record<string, unknown>;
  /** The box to draw while the core catches up, or null when it did not move. */
  box: Rect | null;
}

/** What an item gets when its plugin says nothing: move, scale, rotate. */
export const DEFAULT_GIZMOS: Gizmo[] = [
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
const CURSORS: Record<string, string> = {
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
 * The long spelling, a table per handle with its own anchor and target, is
 * accepted in the same list, so a plugin with a dial on one of its own
 * parameters writes that one out and leaves the rest as names.
 */
export const ARCHETYPES: Record<string, Gizmo[]> = {
  cage: copy(DEFAULT_GIZMOS.slice(0, 1)),
  move: copy(DEFAULT_GIZMOS.slice(0, 1)),
  corner: copy(DEFAULT_GIZMOS.slice(1, 5)),
  edge: copy(DEFAULT_GIZMOS.slice(5, 9)),
  scale: copy(DEFAULT_GIZMOS.slice(1, 9)),
  rotate: copy(DEFAULT_GIZMOS.slice(9, 10)),
  crop: [
    { kind: "edge", anchor: [-0.5, 0], action: "crop", target: "crop.left" },
    { kind: "edge", anchor: [0.5, 0], action: "crop", target: "crop.right" },
    { kind: "edge", anchor: [0, -0.5], action: "crop", target: "crop.top" },
    { kind: "edge", anchor: [0, 0.5], action: "crop", target: "crop.bottom" },
  ],
};

/**
 * An archetype holds its own objects, and its own anchor arrays. A caller that
 * nudges a handle it was handed would otherwise move the defaults for every
 * item in the document, which is a bug nobody would find by reading the caller.
 */
function copy(list: Gizmo[]): Gizmo[] {
  return list.map((g) => ({ ...g, anchor: [g.anchor[0], g.anchor[1]] as [number, number] }));
}

/**
 * The gizmos for one item: the plugin's if it declared any, the defaults if
 * not. Never empty, because an item with no handles cannot be edited at all,
 * which is the one outcome the handle rules rule out.
 */
export function gizmosFor(designer: DesignerBlock | null | undefined): Gizmo[] {
  const declared: Gizmo[] = [];
  for (const entry of (designer && designer.gizmos) || []) {
    if (typeof entry === "string") declared.push(...(ARCHETYPES[entry] || []));
    else if (valid(entry)) declared.push(entry);
  }
  if (!declared.length) return DEFAULT_GIZMOS;
  // A plugin that declares handles still gets to be moved: without a cage
  // nothing could pick the item up, and no plugin should have to remember it.
  return declared.some((g) => g.kind === "cage") ? declared : [DEFAULT_GIZMOS[0]!, ...declared];
}

function valid(g: Gizmo | null | undefined): boolean {
  return !!g && typeof g.kind === "string" && Array.isArray(g.anchor) && g.anchor.length === 2;
}

/**
 * Where each handle sits, in canvas pixels, for one item's box.
 *
 * @param box the derived geometry the core answered with
 */
export function handlesFor(box: Rect, gizmos: Gizmo[] | null | undefined): Handle[] {
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  return (gizmos || []).map((g, i) => {
    const nx = g.anchor[0];
    const ny = g.anchor[1];
    const dir: [number, number] = [Math.sign(nx), Math.sign(ny)];
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
export function hitTest(handles: Handle[], x: number, y: number, r: number): Handle | null {
  let best: Handle | null = null;
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

/** Where the item was when the drag began. */
export interface DragStart {
  box: Rect;
  transform?: Transform | null;
}

/**
 * A drag on one handle, as the props to assign.
 *
 * Pure arithmetic: it is handed where the item is and where the pointer went,
 * and it answers with the `props` for `scene.item.set` and the box to draw
 * while the core catches up. No DOM, no client, no state, so the Python and
 * TypeScript ports are the same function and the fixtures check all three.
 *
 * @param dx canvas pixels moved since the drag began
 */
export function applyDrag(
  handle: Handle,
  start: DragStart,
  dx: number,
  dy: number,
  mods: DragMods = {},
): DragResult {
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
      return rotated(handle, box, mods);
    case "set":
      return dialled(handle, dx, mods);
    default:
      return moved(box, t, dx, dy);
  }
}

function moved(box: Rect, t: Transform, dx: number, dy: number): DragResult {
  const position = { x: num(t.position && t.position.x) + dx, y: num(t.position && t.position.y) + dy };
  return {
    props: { transform: { position } },
    box: { ...box, x: box.x + dx, y: box.y + dy },
  };
}

/**
 * A corner or an edge. The opposite side stays where it is, which is what every
 * drawing program does and what a person expects when they pull a corner.
 */
function scaled(handle: Handle, box: Rect, t: Transform, dx: number, dy: number, mods: DragMods): DragResult {
  const sx = handle.dir[0];
  const sy = handle.dir[1];
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
  return { props: { transform: { frame, position } }, box: { ...box, x, y, width: w, height: h } };
}

/**
 * Crop is normalised 0 to 1 of the content's own pixels, not canvas pixels, so
 * it survives a canvas change (vMix and CasparCG do this; OBS crops in pixels
 * and loses them at 720p).
 */
function cropped(handle: Handle, box: Rect, t: Transform, dx: number, dy: number): DragResult {
  const side = String(handle.target || "").split(".").pop() as keyof Crop;
  const crop: Crop = Object.assign({ left: 0, top: 0, right: 0, bottom: 0 }, t.crop || {});
  const along = side === "left" || side === "right" ? dx / Math.max(1, box.width) : dy / Math.max(1, box.height);
  const sign = side === "right" || side === "bottom" ? -1 : 1;
  const opposite: Record<string, keyof Crop> = { left: "right", right: "left", top: "bottom", bottom: "top" };
  const other = opposite[side];
  crop[side] = clamp(num(crop[side]) + along * sign, 0, 0.95 - num(other === undefined ? undefined : crop[other]));
  return { props: { crop }, box };
}

/**
 * Degrees clockwise about the item's centre, snapped to 15 with Shift.
 * The angle is read from where the pointer is, not from how far it travelled: a
 * rotation handle that integrates deltas drifts over a long drag.
 */
function rotated(handle: Handle, box: Rect, mods: DragMods): DragResult {
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const p = mods.pointer || { x: handle.x, y: handle.y };
  const at = Math.atan2(p.y - cy, p.x - cx) * (180 / Math.PI) + 90;
  let deg = mods.aspect ? Math.round(at / 15) * 15 : at;
  deg = ((deg % 360) + 360) % 360;
  return { props: { transform: { rotation: round(deg, 2) } }, box };
}

/**
 * A dial: sideways travel is the number. The plugin's schema says what the ends
 * are; without one, 0 to 1 over 200 pixels, which is what a person expects of a
 * knob they have never seen.
 */
function dialled(handle: Handle, dx: number, mods: DragMods): DragResult {
  const min = mods.min === undefined ? 0 : mods.min;
  const max = mods.max === undefined ? 1 : mods.max;
  const from = mods.from === undefined ? min : mods.from;
  const span = (max - min) / 200;
  let value = clamp(from + dx * span, min, max);
  if (mods.step) value = Math.round(value / mods.step) * mods.step;
  return { props: pathProps(handle.target, round(value, 4)), box: null };
}

/** `params.key_tolerance` becomes `{params: {key_tolerance: value}}`. */
export function pathProps(path: string | null | undefined, value: unknown): Record<string, unknown> {
  const parts = String(path || "").split(".").filter(Boolean);
  if (!parts.length) return {};
  const out: Record<string, unknown> = {};
  let at = out;
  for (let i = 0; i < parts.length - 1; i += 1) {
    const next: Record<string, unknown> = {};
    at[parts[i]!] = next;
    at = next;
  }
  at[parts[parts.length - 1]!] = value;
  return out;
}

/** Read `transform.frame.w` out of an object, or undefined. */
export function pathValue(object: unknown, path: string | null | undefined): unknown {
  let at: unknown = object;
  for (const part of String(path || "").split(".").filter(Boolean)) {
    if (at === null || at === undefined || typeof at !== "object") return undefined;
    at = (at as Record<string, unknown>)[part];
  }
  return at;
}

function num(v: unknown, fallback?: number): number {
  return typeof v === "number" && isFinite(v) ? v : fallback === undefined ? 0 : fallback;
}

function clamp(v: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, v));
}

function round(v: number, places: number): number {
  const f = Math.pow(10, places);
  return Math.round(v * f) / f;
}

// ------------------------------------------------------------- the snapping
//
// Snapping, split the way tldraw splits it. The plugin answers with geometry:
// `snap = { bounds = true, points = ["params.anchor_point"] }` says this item
// type snaps by its box and by one named point of its own. The client owns
// everything else: the threshold, the modifier that turns snapping off, the
// grid, and the drawing of the guides. That division is why a plugin author
// never tunes a pixel and why a Pi can use a wider threshold than a workstation
// without asking anybody.

/** How close, in canvas pixels, before an edge is taken as lined up. */
export const THRESHOLD = 8;

export type Span = [number, number];

export interface SnapTarget {
  at: number;
  span: Span;
}

export interface SnapTargets {
  v: SnapTarget[];
  h: SnapTarget[];
}

export interface SnapInput {
  canvas?: Size | null;
  boxes?: Rect[];
  points?: Point[];
  grid?: number;
}

export interface Guide {
  axis: "v" | "h";
  at: number;
  span: Span;
}

export interface SnapOptions {
  /** The modifier held: snapping is on by default and the modifier turns it off. */
  invert?: boolean;
  threshold?: number;
  grid?: number;
}

export interface SnapDelta {
  dx: number;
  dy: number;
  guides: Guide[];
}

/** Every line a moving box could line up with. */
export function snapTargets(what: SnapInput): SnapTargets {
  const v: SnapTarget[] = [];
  const h: SnapTarget[] = [];
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
    const span: Span = [box.y, box.y + box.height];
    push(v, box.x, span);
    push(v, box.x + box.width / 2, span);
    push(v, box.x + box.width, span);
    const across: Span = [box.x, box.x + box.width];
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

function push(list: SnapTarget[], at: number, span: Span): void {
  if (!isFinite(at)) return;
  const had = list.find((t) => Math.abs(t.at - at) < 0.01);
  if (had) {
    had.span = [Math.min(had.span[0], span[0]), Math.max(had.span[1], span[1])];
    return;
  }
  list.push({ at, span: [span[0], span[1]] });
}

/** How far to nudge a box so it lines up, and what to draw while it does. */
export function snapDelta(box: Rect, targets: SnapTargets, opts: SnapOptions = {}): SnapDelta {
  const out: SnapDelta = { dx: 0, dy: 0, guides: [] };
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

function nearest(
  edges: number[],
  targets: SnapTarget[] | null | undefined,
  threshold: number,
): { delta: number; target: SnapTarget } | null {
  let best: { delta: number; target: SnapTarget } | null = null;
  for (const edge of edges) {
    for (const target of targets || []) {
      const delta = target.at - edge;
      if (Math.abs(delta) > threshold) continue;
      if (!best || Math.abs(delta) < Math.abs(best.delta)) best = { delta, target };
    }
  }
  return best;
}

function grown(span: Span, other: Span): Span {
  return [Math.min(span[0], other[0]), Math.max(span[1], other[1])];
}

/** What a plugin's `snap` block declares. */
export interface SnapBlock {
  bounds?: boolean;
  points?: string[];
}

/**
 * The points a plugin's `snap.points` names, resolved against one item.
 * A point is normalised within the item's own box, because a plugin knows its
 * anchor as a fraction and knows nothing about the canvas.
 */
export function pointsOf(item: unknown, box: Rect, snap: SnapBlock | null | undefined): Point[] {
  const names = (snap && snap.points) || [];
  const out: Point[] = [];
  for (const path of names) {
    const value = read(item, path);
    if (!value || typeof value !== "object") continue;
    const x = Number((value as Record<string, unknown>)["x"]);
    const y = Number((value as Record<string, unknown>)["y"]);
    if (!isFinite(x) || !isFinite(y)) continue;
    out.push({ x: box.x + x * box.width, y: box.y + y * box.height });
  }
  return out;
}

function read(object: unknown, path: string | null | undefined): unknown {
  let at: unknown = object;
  for (const part of String(path || "").split(".").filter(Boolean)) {
    if (!at || typeof at !== "object") return undefined;
    at = (at as Record<string, unknown>)[part];
  }
  return at;
}

// ------------------------------------------------------ safe areas and rulers
//
// The same two numbers the core's validator uses (`validate.rs`): action safe
// is the middle 93 percent, title safe the middle 90. Drawing them with
// different numbers from the ones `scene.validate` warns about would be worse
// than not drawing them at all, so they are named here once and the fixtures
// check the ports against the same pair.

export const ACTION_SAFE = 0.07;
export const TITLE_SAFE = 0.1;

export interface SafeAreas {
  action: Rect;
  title: Rect;
}

/** Shrink a box towards its centre by a fraction of each side. */
export function insetFraction(box: Rect, fraction: number): Rect {
  const dx = (box.width * fraction) / 2;
  const dy = (box.height * fraction) / 2;
  return { x: box.x + dx, y: box.y + dy, width: box.width - dx * 2, height: box.height - dy * 2 };
}

/** The two rectangles a designer draws over the picture, in canvas pixels. */
export function safeAreas(canvas: Size): SafeAreas {
  const full: Rect = { x: 0, y: 0, width: canvas.width, height: canvas.height };
  return {
    action: insetFraction(full, ACTION_SAFE),
    title: insetFraction(full, TITLE_SAFE),
  };
}

/**
 * Ruler ticks along one axis, at a spacing a person reads rather than a round
 * number of pixels: about one tick per 80 screen pixels, rounded to 10, 20, 50,
 * 100 and so on, so the numbers stay legible at any zoom.
 */
export function ticks(length: number, scale: number, wanted = 80): number[] {
  const raw = wanted / (scale || 1);
  const power = Math.pow(10, Math.floor(Math.log10(Math.max(1, raw))));
  const step = [1, 2, 5, 10].map((m) => m * power).find((s) => s >= raw) || power * 10;
  const out: number[] = [];
  for (let at = 0; at <= length + 0.5; at += step) out.push(Math.round(at));
  return out;
}
