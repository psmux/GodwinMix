// The demo's PTZ heads: where each camera points, how a joystick velocity,
// a zoom, a preset or the follow loop moves it. Same rule as the real ptz
// plugin: a velocity stops by itself 300 ms after the last call.

import { FLOOR, seenX } from "./world.js";

export const STOP_AFTER_MS = 300;

const SIZE = { close: 0.55, waist: 0.85, full: 1.25, wide: 2.2 };

export function camera(id, name, view, look) {
  return {
    id, name, source: id, model: "Demo PTZ", connected: true, look: look || {},
    view: { ...view }, goal: null,
    vel: { pan: 0, tilt: 0, zoom: 0, at: -1e9, zoomAt: -1e9 },
    tracking: { on: false, person: null, style: "waist", handUntil: 0 },
    presets: [], exposure: { mode: "auto", iris: 11, shutter: 7, gain: 0, brightness: 0 }, focus: { mode: "auto" },
  };
}

const clamp = (v, lo, hi) => Math.max(lo, Math.min(hi, v));

export function clampView(v) {
  v.w = clamp(v.w, 0.14, 1);
  v.cx = clamp(v.cx, v.w / 2, 1 - v.w / 2);
  v.cy = clamp(v.cy, v.w / 2, 1 - v.w / 2);
  return v;
}

/** The view that frames person `p` in a style, with lead room the way they walk. */
export function framed(cam, p, style) {
  const w = p.h * (SIZE[style] || SIZE.waist);
  const lead = p.wait > 0 ? 0 : p.dir * 0.08 * w * (cam.look.mirror ? -1 : 1);
  return clampView({ cx: seenX(cam, p.x) + lead, cy: FLOOR - p.h + 0.4 * w, w });
}

function ease(v, goal, k) {
  v.cx += (goal.cx - v.cx) * k;
  v.cy += (goal.cy - v.cy) * k;
  v.w += (goal.w - v.w) * k;
}

/** Move one camera on by `dt` seconds at time `now` (ms). */
export function stepCamera(cam, w, dt, now) {
  const v = cam.view;
  const byHand = now - cam.vel.at < STOP_AFTER_MS;
  if (byHand) {
    v.cx += cam.vel.pan * dt * 0.4 * v.w;
    v.cy -= cam.vel.tilt * dt * 0.4 * v.w;
    cam.tracking.handUntil = now + 1500;
    cam.goal = null;
  }
  if (now - cam.vel.zoomAt < STOP_AFTER_MS) {
    v.w *= Math.exp(-cam.vel.zoom * dt * 0.9);
    cam.goal = null;
  }
  const t = cam.tracking;
  const person = t.on && t.person && w.people.find((p) => p.id === t.person);
  if (person && now > t.handUntil) ease(v, framed(cam, person, t.style), Math.min(1, dt * 2.4));
  else if (cam.goal) {
    ease(v, cam.goal, Math.min(1, dt * 3));
    if (Math.abs(v.cx - cam.goal.cx) + Math.abs(v.w - cam.goal.w) < 0.002) cam.goal = null;
  }
  clampView(v);
}
