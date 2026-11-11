// The demo's follow loop as the ptz plugin reports it: the same JSON that
// plugins/ptz/src/follow/view.rs builds, so the window reads the demo and a
// real camera through one path in shapes.js.

import { observe } from "./world.js";

/** An observing loop lingers this long after the last ask, as the plugin's does. */
export const LINGER_MS = 15000;

const box = ([x, y, w, h]) => ({ x, y, w, h });

/** Is a loop running for this camera: tracking, or asked to look lately. */
export const looking = (cam, now) => cam.tracking.on || now - (cam.observedAt ?? -1e9) < LINGER_MS;

export function seen(w, cam, now) {
  const t = cam.tracking;
  const people = observe(w, cam);
  const target = t.on ? t.person : null;
  const inView = people.some((p) => p.id === target);
  return {
    camera: cam.id,
    mode: t.on ? "following" : "observing",
    state: !t.on ? "idle" : !target ? "idle" : inView ? "following" : "holding",
    framing: t.style,
    target,
    tracks: people.map((p) => ({ id: p.id, box: box(p.box), confidence: p.score, state: "confirmed", followed: p.id === target })),
    shot: null,
    ptz: { pan: cam.vel.pan, tilt: cam.vel.tilt, zoom: cam.vel.zoom },
    frames: Math.round(now / 80),
    fps: 12.5,
    step_ms: 3.1,
    picture: { width: 320, height: 180, from: "demo" },
    age_ms: 40,
    error: null,
  };
}

/** track_state's answer: the loop, or a camera with nothing running. */
export function state(w, cam, now) {
  return looking(cam, now) ? seen(w, cam, now) : { camera: cam.id, mode: "stopped", tracks: [] };
}

/** Who `target` or `point` means, as the plugin's track_follow reads them. */
export function choose(w, cam, a) {
  const people = observe(w, cam);
  const mid = (p) => [p.box[0] + p.box[2] / 2, p.box[1] + p.box[3] / 2];
  const near = (x, y) => people.slice().sort((a, b) => Math.hypot(mid(a)[0] - x, mid(a)[1] - y) - Math.hypot(mid(b)[0] - x, mid(b)[1] - y))[0];
  if (a.point) return (near(a.point.x, a.point.y) || {}).id || null;
  if (a.target === "largest") return (people.slice().sort((p, q) => q.box[3] - p.box[3])[0] || {}).id || null;
  if (a.target === "central" || !a.target) return (near(0.5, 0.5) || {}).id || cam.tracking.person;
  return a.target;
}
