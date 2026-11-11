// The round joystick: drag the knob and the camera turns that way, faster
// the further out it goes; let go and it springs back to the middle and the
// camera stops. The arrow keys do the same while it has focus.

import { el } from "../../shell/dom.js";

const KEYS = { ArrowLeft: [-1, 0], ArrowRight: [1, 0], ArrowUp: [0, 1], ArrowDown: [0, -1] };

/** `drive` takes move(pan, tilt) and stopAll(). Answers {root, release}. */
export function joystick(drive) {
  const knob = el("div.cam-knob", { "aria-hidden": "true" });
  const root = el("div.cam-stick", {
    role: "slider",
    tabindex: "0",
    "aria-label": "Point the camera. Drag, or hold the arrow keys",
    "aria-valuetext": "Still",
  }, [
    el("div.cam-stick-ring"),
    ...["up", "right", "down", "left"].map((d) => el(`div.cam-stick-arrow.${d}`, { "aria-hidden": "true" })),
    knob,
  ]);
  let id = null;
  const held = new Set();

  const place = (x, y, spring) => {
    knob.classList.toggle("spring", !!spring);
    const reach = (root.clientWidth || 180) * 0.31;
    knob.style.transform = `translate(calc(-50% + ${x * reach}px), calc(-50% + ${-y * reach}px))`;
    root.setAttribute("aria-valuetext", x || y ? `Pan ${Math.round(x * 100)}, tilt ${Math.round(y * 100)}` : "Still");
  };

  const at = (e) => {
    const r = root.getBoundingClientRect();
    const rad = r.width / 2;
    let x = (e.clientX - r.left - rad) / (rad * 0.62);
    let y = -(e.clientY - r.top - rad) / (rad * 0.62);
    const len = Math.hypot(x, y);
    if (len > 1) (x /= len), (y /= len);
    // A gentle curve: fine moves near the middle, full speed at the edge.
    const curve = (v) => Math.sign(v) * Math.pow(Math.abs(v), 1.6);
    place(x, y);
    drive.move(curve(x), curve(y));
  };

  const release = () => {
    id = null;
    held.clear();
    root.classList.remove("held");
    place(0, 0, true);
    drive.stopAll();
  };

  root.addEventListener("pointerdown", (e) => {
    if (id !== null) return;
    id = e.pointerId;
    root.setPointerCapture?.(id);
    root.classList.add("held");
    e.preventDefault();
    at(e);
  });
  root.addEventListener("pointermove", (e) => e.pointerId === id && at(e));
  for (const t of ["pointerup", "pointercancel", "lostpointercapture"]) root.addEventListener(t, (e) => e.pointerId === id && release());

  const keys = () => {
    let x = 0;
    let y = 0;
    for (const k of held) (x += KEYS[k][0] * 0.5), (y += KEYS[k][1] * 0.5);
    place(x, y, !held.size);
    held.size ? drive.move(x, y) : drive.stopAll();
  };
  root.addEventListener("keydown", (e) => {
    if (!KEYS[e.key]) return;
    e.preventDefault();
    if (!held.has(e.key)) held.add(e.key), keys();
  });
  root.addEventListener("keyup", (e) => KEYS[e.key] && (held.delete(e.key), keys()));
  root.addEventListener("blur", () => held.size && release());
  place(0, 0);
  return { root, release, place };
}
