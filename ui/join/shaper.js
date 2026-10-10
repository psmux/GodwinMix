// The camera through a canvas of a fixed shape, which is what /join/ sends.
//
// Every frame is drawn onto a canvas the size of the chosen shape, turned
// upright and filled or fitted, and the canvas's own track goes on the wire.
// So the mixer gets one size for as long as the publish lasts, whatever the
// phone does: turned, locked, flipped to the other camera. The canvas is the
// preview as well, so a person sees exactly what is sent.
//
// It costs one 2D draw a frame, which every phone that can run WebRTC does
// without noticing. Only /join/ uses it; a laptop's camera on the mixer's own
// page goes straight on the wire as before.

import { SHAPES, correction, deviceTurn, placement, quarter, screenTurn } from "./shape.js";
import { facingOf } from "./flip.js";

const FPS = 30;

/** iOS asks before a page may read the motion sensor, and only on a tap. */
export function askMotion() {
  const ask = typeof DeviceOrientationEvent !== "undefined" && DeviceOrientationEvent.requestPermission;
  if (typeof ask === "function") ask.call(DeviceOrientationEvent).catch(() => {});
}

function screenAngle() {
  const o = typeof screen !== "undefined" && screen.orientation;
  if (o && typeof o.angle === "number") return o.angle;
  return Number(window.orientation) || 0;
}

export class Shaper {
  /** `opts`: `{shape, fit, auto}`. */
  constructor(opts = {}) {
    this.s = { shape: opts.shape || "landscape", fit: opts.fit || "fill", auto: opts.auto !== false, manual: 0, device: null, facing: "" };
    this.canvas = document.createElement("canvas");
    this.canvas.className = "pub-canvas";
    this.ctx = this.canvas.getContext("2d", { alpha: false });
    // In the page but out of sight: iOS will not decode a video that is not.
    this.video = Object.assign(document.createElement("video"), { muted: true, playsInline: true, autoplay: true });
    this.video.className = "pub-source";
    this.video.setAttribute("aria-hidden", "true");
    this.size();
    this.track = this.canvas.captureStream(FPS).getVideoTracks()[0] || null;
    this.onTilt = (e) => (this.s.device = deviceTurn(e.beta, e.gamma, this.s.device));
    window.addEventListener("deviceorientation", this.onTilt);
    this.running = true;
    this.loop();
  }

  /** The camera to draw, or none. */
  setTrack(track) {
    this.s.facing = facingOf(track);
    this.video.srcObject = track ? new MediaStream([track]) : null;
    if (track) this.video.play().catch(() => {});
  }

  /** Change any of `shape`, `fit`, `auto`. */
  set(changes) {
    Object.assign(this.s, changes);
    this.size();
    this.draw();
  }

  /** A quarter turn clockwise on top of whatever the sensor says. */
  rotate() {
    this.s.manual = quarter(this.s.manual + 90);
    this.draw();
  }

  /** The turn applied now, for the controls to show. */
  turn() {
    return correction({ device: this.s.device, screen: screenTurn(screenAngle()), facing: this.s.facing, manual: this.s.manual, auto: this.s.auto });
  }

  size() {
    const { w, h } = SHAPES[this.s.shape] || SHAPES.landscape;
    if (this.canvas.width !== w) this.canvas.width = w;
    if (this.canvas.height !== h) this.canvas.height = h;
  }

  draw() {
    const { width: ow, height: oh } = this.canvas;
    const ctx = this.ctx;
    ctx.fillStyle = "#000";
    ctx.fillRect(0, 0, ow, oh);
    const v = this.video;
    const turn = this.turn();
    const p = v.srcObject && v.readyState >= 2 ? placement(v.videoWidth, v.videoHeight, turn, ow, oh, this.s.fit) : null;
    if (!p) return;
    ctx.save();
    ctx.translate(ow / 2, oh / 2);
    ctx.rotate((turn * Math.PI) / 180);
    ctx.drawImage(v, -p.w / 2, -p.h / 2, p.w, p.h);
    ctx.restore();
  }

  /**
   * Each new camera frame where the browser says when there is one, else
   * thirty a second. The timer stands behind the frame callback as well,
   * which never comes for a camera taken away while it was waiting.
   */
  loop() {
    if (!this.running) return;
    this.draw();
    const gen = (this.gen = (this.gen || 0) + 1);
    const next = () => gen === this.gen && this.loop();
    clearTimeout(this.timer);
    const v = this.video;
    if (v.requestVideoFrameCallback && v.srcObject) {
      v.requestVideoFrameCallback(next);
      this.timer = setTimeout(next, 250);
    } else {
      this.timer = setTimeout(next, 1000 / FPS);
    }
  }

  destroy() {
    this.running = false;
    clearTimeout(this.timer);
    window.removeEventListener("deviceorientation", this.onTilt);
    this.video.srcObject = null;
    if (this.track) this.track.stop();
  }
}
