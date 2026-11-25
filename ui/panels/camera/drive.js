// Sends joystick and zoom speeds to a camera as one ptz `move`, no more
// than RATE times a second, and zero speeds when the hand comes off. The
// plugin holds each move for 300 ms and then stops the camera by itself, so
// a held stick is sent again at least every RESEND ms even when it has not
// moved; a closed tab or a lost network can never leave a camera turning.
//
// Letting go sends zero speeds rather than the plugin's `stop`, because
// `stop` also turns tracking off, and a nudge by hand should not.
//
// It also lets go on window blur and on page hide, because a pointer that
// is down when the window goes away never sends its pointerup.

export const RATE = 15;
export const RESEND = 150;
const DEAD = 0.04;

const round = (v) => Math.round(Math.max(-1, Math.min(1, v)) * 100) / 100;
const zero = () => ({ pan: 0, tilt: 0, zoom: 0 });
const same = (a, b) => a.pan === b.pan && a.tilt === b.tilt && a.zoom === b.zoom;
const moving = (v) => !!(v.pan || v.tilt || v.zoom);

export class Drive {
  /** `send` is {move(cam, pan, tilt, zoom), still(cam)}; `now` is for tests. */
  constructor(send, now = () => performance.now()) {
    this.send = send;
    this.now = now;
    this.camera = null;
    this.want = zero();
    this.sent = zero();
    this.at = -1e9;
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
    if (!this.timer && moving(this.want)) this.timer = setInterval(() => this.tick(), 1000 / RATE);
  }

  /** One slot of the rate: send what changed, or what is due again. */
  tick() {
    const cam = this.camera;
    if (!cam) return;
    const now = this.now();
    const w = { ...this.want };
    const gap = now - this.at;
    const due = !same(w, this.sent) || (moving(w) && gap >= RESEND);
    if (due && gap >= 1000 / RATE - 2) {
      if (moving(w)) this.fire(() => this.send.move(cam, w.pan, w.tilt, w.zoom));
      else this.fire(() => this.send.still(cam));
      this.sent = w;
      this.at = now;
    }
    if (!moving(this.want) && !moving(this.sent)) this.idle();
  }

  fire(fn) {
    Promise.resolve().then(fn).catch((e) => this.onError && this.onError(e));
  }

  idle() {
    clearInterval(this.timer);
    this.timer = null;
  }

  /** Hands off: zero speeds now, whatever the rate says. */
  stopAll() {
    const cam = this.camera;
    const was = moving(this.sent) || moving(this.want);
    this.want = zero();
    this.idle();
    if (cam && was) this.fire(() => this.send.still(cam));
    this.sent = zero();
    this.at = this.now();
  }

  destroy() {
    this.stopAll();
    window.removeEventListener("blur", this.halt);
    window.removeEventListener("pagehide", this.halt);
    document.removeEventListener("visibilitychange", this.hidden);
  }
}
