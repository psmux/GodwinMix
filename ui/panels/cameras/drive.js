// Sends joystick and zoom velocities to a camera, no more than RATE times a
// second, and always a stop when the hand comes off. The ptz plugin stops a
// camera by itself 300 ms after the last velocity, so a held stick is sent
// again at least every RESEND ms even when it has not moved; a closed tab or
// a lost network can never leave a camera turning.
//
// It also stops on window blur and on page hide, because a pointer that is
// down when the window goes away never sends its pointerup.

export const RATE = 15;
export const RESEND = 150;
const DEAD = 0.04;

const round = (v) => Math.round(Math.max(-1, Math.min(1, v)) * 100) / 100;

export class Drive {
  /** `send` is {move(cam, pan, tilt), stop(cam), zoom(cam, speed)}; `now` is for tests. */
  constructor(send, now = () => performance.now()) {
    this.send = send;
    this.now = now;
    this.camera = null;
    this.want = { pan: 0, tilt: 0, zoom: 0 };
    this.sent = { pan: 0, tilt: 0, zoom: 0, moveAt: -1e9, zoomAt: -1e9 };
    this.timer = null;
    this.halt = () => this.stopAll();
    this.hidden = () => document.hidden && this.stopAll();
    window.addEventListener("blur", this.halt);
    window.addEventListener("pagehide", this.halt);
    document.addEventListener("visibilitychange", this.hidden);
  }

  use(camera) {
    if (camera !== this.camera) this.stopAll();
    this.camera = camera;
  }

  /** The stick at pan and tilt, each -1 to 1. */
  move(pan, tilt) {
    this.want.pan = Math.abs(pan) < DEAD ? 0 : round(pan);
    this.want.tilt = Math.abs(tilt) < DEAD ? 0 : round(tilt);
    this.kick();
  }

  zoom(speed) {
    this.want.zoom = Math.abs(speed) < DEAD ? 0 : round(speed);
    this.kick();
  }

  kick() {
    this.tick();
    if (!this.timer && this.active()) this.timer = setInterval(() => this.tick(), 1000 / RATE);
  }

  active() {
    return !!(this.want.pan || this.want.tilt || this.want.zoom);
  }

  /** One slot of the rate: send what changed, or what is due again. */
  tick() {
    const cam = this.camera;
    if (!cam) return;
    const now = this.now();
    const w = this.want;
    const s = this.sent;
    const minGap = 1000 / RATE - 2;
    const moving = w.pan || w.tilt;
    const moveChanged = w.pan !== s.pan || w.tilt !== s.tilt;
    if (now - s.moveAt >= minGap && (moveChanged ? true : moving && now - s.moveAt >= RESEND)) {
      if (moving) this.fire(() => this.send.move(cam, w.pan, w.tilt));
      else this.fire(() => this.send.stop(cam));
      Object.assign(s, { pan: w.pan, tilt: w.tilt, moveAt: now });
    }
    if (now - s.zoomAt >= minGap && (w.zoom !== s.zoom || (w.zoom && now - s.zoomAt >= RESEND))) {
      this.fire(() => this.send.zoom(cam, w.zoom));
      Object.assign(s, { zoom: w.zoom, zoomAt: now });
    }
    if (!this.active() && s.pan === 0 && s.tilt === 0 && s.zoom === 0) this.idle();
  }

  fire(fn) {
    Promise.resolve().then(fn).catch((e) => this.onError && this.onError(e));
  }

  idle() {
    clearInterval(this.timer);
    this.timer = null;
  }

  /** Hands off: stop now, whatever the rate says. */
  stopAll() {
    const cam = this.camera;
    const was = this.sent.pan || this.sent.tilt || this.sent.zoom || this.active();
    this.want = { pan: 0, tilt: 0, zoom: 0 };
    this.idle();
    if (cam && was) {
      this.fire(() => this.send.stop(cam));
      if (this.sent.zoom) this.fire(() => this.send.zoom(cam, 0));
    }
    this.sent = { pan: 0, tilt: 0, zoom: 0, moveAt: this.now(), zoomAt: this.now() };
  }

  destroy() {
    this.stopAll();
    window.removeEventListener("blur", this.halt);
    window.removeEventListener("pagehide", this.halt);
    document.removeEventListener("visibilitychange", this.hidden);
  }
}
