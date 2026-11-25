// Scroll from the shared state, one frame at a time.
//
// The frame loop runs only while the prompter plays, or while a jump is
// being eased out, and only while the page is visible. A paused prompter
// draws once and then costs nothing.
//
// A new state (a speed change, a nudge from another screen) moves the target.
// A small difference is eased out over a quarter of a second so the words do
// not jerk; a jump of more than a few rows is a seek and is drawn at once.

import { positionAt } from "./clock.js";

const EASE_S = 0.25;
const SNAP_ROWS = 3;

export class Follower {
  constructor(stage, clock) {
    this.stage = stage;
    this.clock = clock;
    this.state = null;
    this.error = 0;
    this.shown = 0;
    this.last = 0;
    this.frame = 0;
    this.tick = (t) => this.step(t);
    document.addEventListener("visibilitychange", () => this.wake());
  }

  /** Where every screen agrees the prompter is now. */
  target() {
    return positionAt(this.state, this.clock.now());
  }

  /** A new state from the mixer. */
  apply(state) {
    const before = this.state ? this.shown : null;
    this.state = state;
    const now = this.target();
    const gap = before === null ? 0 : before - now;
    this.error = Math.abs(gap) < SNAP_ROWS ? gap : 0;
    this.wake();
  }

  /** The clock was set again: start from the target, not from the old error. */
  resync() {
    this.error = 0;
    this.wake();
  }

  wake() {
    if (this.frame || document.visibilityState === "hidden") return;
    this.last = 0;
    this.frame = requestAnimationFrame(this.tick);
  }

  step(t) {
    this.frame = 0;
    const dt = this.last ? (t - this.last) / 1000 : 0;
    this.last = t;
    this.error *= Math.exp(-dt / EASE_S);
    if (Math.abs(this.error) < 0.002) this.error = 0;
    this.shown = this.target() + this.error;
    this.stage.draw(this.shown);
    const moving = this.state && this.state.playing && this.state.speed !== 0;
    if ((moving || this.error) && document.visibilityState !== "hidden") {
      this.frame = requestAnimationFrame(this.tick);
    }
  }
}
