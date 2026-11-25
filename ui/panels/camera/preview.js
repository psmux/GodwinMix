// The camera over Studio mode's Preview monitor: the people the tracker sees
// boxed on the Preview picture (tap one to follow them), and a small strip at
// its foot with the joystick, zoom, framing chips and Track, which folds down
// to one camera button. Only ever on Preview: the Programme monitor gets
// none of it, so what is on air never moves by accident.
//
// ../multiview/studio-camera.js fetches this the first time Preview holds a
// camera and calls show(camera or null) whenever that changes.

import { el } from "../../shell/dom.js";
import { run } from "../../shell/commands.js";
import { Drive } from "./drive.js";
import { controls } from "./controls.js";
import { livePicture } from "./live.js";
import { gamepads } from "./gamepad.js";
import { styled, actor, cameraIcon } from "./look.js";
import { trackWords } from "./words.js";

const KEY = "gmx.cameras.overlay";
const folded = () => {
  try {
    return localStorage.getItem(KEY) === "folded";
  } catch {
    return false;
  }
};

/** Where the picture is drawn inside a canvas that fits it with object-fit: contain. */
export function pictureBox(canvas, frame) {
  const c = canvas.getBoundingClientRect();
  const f = frame.getBoundingClientRect();
  const aspect = canvas.width / canvas.height || 16 / 9;
  const w = Math.min(c.width, c.height * aspect);
  const h = w / aspect;
  return { left: c.left - f.left + (c.width - w) / 2, top: c.top - f.top + (c.height - h) / 2, width: w, height: h };
}

export function previewOverlay(panel, hub) {
  styled();
  const frame = panel.previewWrap.querySelector(".preview-frame");
  const act = actor(hub);
  let cam = null;
  let offPeople = null;
  const drive = new Drive({ move: (c, p, t, z) => hub.api.move(c, p, t, z), still: (c) => hub.api.still(c) });
  drive.onError = act.fail;
  const hand = controls(hub, () => cam && cam.id, drive, act);
  const boxes = livePicture(hub, () => cam, act, { picture: false });
  boxes.canvas.classList.add("cam-boxes");
  const name = el("span.cam-ov-name");
  // On a Preview too small for the strip (a phone), the button opens camera
  // properties instead, which has room for every control.
  const full = () => cam && import("./props.js").then((m) => m.openCamera(hub.real, (hub.real.store && hub.real.store.source(cam.source)) || { id: cam.source }));
  const toggle = el("button.btn.icon.cam-ov-toggle", { type: "button", "aria-expanded": "true",
    onclick: () => (root.classList.contains("mini") ? full() : fold(!root.classList.contains("folded"))) }, [cameraIcon(16)]);
  const body = el("div.cam-ov-body", {}, [hand.stick.root, el("div.cam-ov-side", {}, [hand.zoom.root, hand.chips, el("div.row", {}, [hand.track.root, name])])]);
  const root = el("div.cam-ov", { hidden: true, role: "group", "aria-label": "Camera in Preview", ondblclick: (e) => e.stopPropagation() }, [toggle, body]);
  frame.append(boxes.canvas, root);
  const pad = gamepads({
    move: (p, t) => drive.move(p, t),
    zoom: (v) => drive.zoom(v),
    stop: () => drive.stopAll(),
    take: () => run("program.take-armed"),
    cut: () => run("program.cut-armed"),
  });

  function fold(on) {
    root.classList.toggle("folded", on);
    toggle.setAttribute("aria-expanded", String(!on));
    toggle.title = on ? "Show the camera controls" : "Fold the camera controls away";
    try {
      localStorage.setItem(KEY, on ? "folded" : "open");
    } catch {
      /* remembered for this page only */
    }
    if (on) hand.release();
  }

  function fit() {
    if (!cam) return;
    root.classList.toggle("mini", frame.clientWidth < 360);
    const box = pictureBox(panel.previewCanvas, frame);
    boxes.canvas.hidden = !(box.width > 0 && box.height > 0);
    Object.assign(boxes.canvas.style, { left: `${box.left}px`, top: `${box.top}px`, width: `${box.width}px`, height: `${box.height}px` });
  }

  function sync() {
    if (!cam) return;
    const t = hub.track.get(cam.id);
    hand.sync(t);
    name.textContent = trackWords(t);
    fit();
  }

  const offHub = hub.on(sync);
  const ro = new ResizeObserver(fit);
  ro.observe(frame);
  fold(folded());

  return {
    show(next) {
      const id = next ? next.id : null;
      if ((cam && cam.id) === id) return sync();
      offPeople?.();
      offPeople = null;
      hand.release();
      drive.stopAll();
      cam = next;
      root.hidden = boxes.canvas.hidden = !cam;
      if (!cam) return (boxes.stop(), pad.stop());
      drive.use(cam.id);
      offPeople = hub.want("people:" + cam.id);
      toggle.title = "Camera controls";
      boxes.start();
      pad.start();
      sync();
    },
    destroy() {
      this.show(null);
      offHub();
      ro.disconnect();
      drive.destroy();
      pad.destroy();
      boxes.canvas.remove();
      root.remove();
    },
  };
}
