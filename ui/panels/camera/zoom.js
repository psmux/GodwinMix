// The zoom rocker: slide the thumb toward Closer or Wider and the camera
// zooms, faster the further it goes, and springs back to stop on release.
// Holding either end does the same at a steady speed.

import { el } from "../../shell/dom.js";

export function zoomRocker(drive) {
  const thumb = el("div.cam-zoom-thumb", { "aria-hidden": "true" });
  const track = el("div.cam-zoom-track", {}, [thumb]);
  const wider = el("button.cam-zoom-end", { type: "button", text: "Wider", "aria-label": "Zoom out, hold" });
  const closer = el("button.cam-zoom-end", { type: "button", text: "Closer", "aria-label": "Zoom in, hold" });
  const root = el("div.cam-zoom", { role: "group", "aria-label": "Zoom" }, [wider, track, closer]);
  let id = null;

  const show = (v, spring) => {
    thumb.classList.toggle("spring", !!spring);
    thumb.style.transform = `translateX(calc(-50% + ${v * (track.clientWidth || 160) * 0.42}px))`;
  };
  const off = () => {
    id = null;
    show(0, true);
    drive.zoom(0);
    drive.stopAll();
  };
  const at = (e) => {
    const r = track.getBoundingClientRect();
    const v = Math.max(-1, Math.min(1, ((e.clientX - r.left) / r.width - 0.5) / 0.42));
    show(v);
    drive.zoom(v);
  };
  track.addEventListener("pointerdown", (e) => {
    id = e.pointerId;
    try {
      track.setPointerCapture(id);
    } catch {
      /* fine without capture */
    }
    e.preventDefault();
    at(e);
  });
  track.addEventListener("pointermove", (e) => e.pointerId === id && at(e));
  for (const t of ["pointerup", "pointercancel", "lostpointercapture"]) track.addEventListener(t, (e) => e.pointerId === id && off());

  const hold = (btn, v) => {
    btn.addEventListener("pointerdown", (e) => {
      try {
        btn.setPointerCapture(e.pointerId);
      } catch {
        /* fine without capture */
      }
      show(v * 0.7);
      drive.zoom(v * 0.7);
    });
    for (const t of ["pointerup", "pointercancel", "lostpointercapture"]) btn.addEventListener(t, off);
    btn.addEventListener("keydown", (e) => (e.key === " " || e.key === "Enter") && !e.repeat && (e.preventDefault(), drive.zoom(v * 0.7)));
    btn.addEventListener("keyup", (e) => (e.key === " " || e.key === "Enter") && off());
  };
  hold(wider, -1);
  hold(closer, 1);
  show(0);
  return { root, release: off };
}
