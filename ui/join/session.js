// A publish that stays up: connect, watch the connection, and when it fails
// publish again after a wait that grows, until somebody presses Stop.
//
// The states a person sees: connecting, live, reconnecting, stopped. A
// refusal that waiting will not cure (a wrong key, a browser with no codec
// the mixer takes) stops at once with the mixer's own sentence; `retry.js`
// has the list. The network coming back or changing, and the page being
// shown again, try at once rather than at the end of the wait (`nudge`).

import { publish, unpublish } from "./whip.js";
import { backoff, isFinal, GRACE_MS, MOVED_MS } from "./retry.js";

export { backoff, isFinal };

export class Session {
  /**
   * `opts`: `{url, key, tracks?, onChange?, connect?, end?}`. `onChange` hears
   * `{state, error, retryIn}`; `connect` and `end` are the WHIP calls,
   * swappable so the tests need no network.
   */
  constructor(opts) {
    this.opts = opts;
    this.tracks = { video: null, audio: null, ...(opts.tracks || {}) };
    this.connect = opts.connect || publish;
    this.end = opts.end || unpublish;
    this.state = "idle";
    this.error = "";
    this.retryIn = 0;
    this.failures = 0;
    this.gen = 0;
    this.live = null;
    this.stopped = false;
    this.pending = false;
    this.moved = -Infinity;
  }

  start() {
    this.stopped = false;
    this.failures = 0;
    this.attempt();
  }

  stop() {
    this.stopped = true;
    this.gen += 1;
    this.clearTimers();
    this.drop();
    this.error = "";
    this.set("stopped");
  }

  /** A new camera or microphone, or none. Live senders change without renegotiating. */
  setTrack(kind, track) {
    this.tracks[kind] = track || null;
    const sender = this.live && this.live.senders[kind];
    return sender ? sender.replaceTrack(track || null) : Promise.resolve();
  }

  /** True while there is something on the wire, or about to be. */
  get active() {
    return this.state === "connecting" || this.state === "live" || this.state === "reconnecting";
  }

  /**
   * The network came back or changed, or the page was shown again. A wait
   * for the next try ends now; a connection that has lost its path is
   * replaced now; one that is up, or an offer already on its way, is left.
   * `moved` says the network itself changed (Wi-Fi to cellular).
   */
  nudge(moved = false) {
    if (moved) this.moved = Date.now();
    if (this.stopped || !this.active || this.pending) return;
    const s = this.live && this.live.pc.connectionState;
    if (s === "connected" || s === "new" || s === "connecting") return;
    this.clearTimers();
    this.drop();
    this.attempt();
  }

  async attempt() {
    if (this.stopped) return;
    const gen = ++this.gen;
    this.retryIn = 0;
    this.pending = true;
    this.set(this.failures ? "reconnecting" : "connecting");
    let live;
    try {
      live = await this.connect({ url: this.opts.url, key: this.opts.key, media: { ...this.tracks } });
    } catch (e) {
      if (gen === this.gen) this.pending = false;
      return this.fail(e, gen);
    }
    if (this.stopped || gen !== this.gen) {
      live.pc.close();
      this.end(live.location);
      return;
    }
    this.pending = false;
    this.live = live;
    this.watch(live.pc, gen);
  }

  watch(pc, gen) {
    const check = () => {
      if (gen !== this.gen) return;
      const s = pc.connectionState;
      clearTimeout(this.grace);
      if (s === "connected") {
        this.failures = 0;
        this.error = "";
        this.set("live");
      } else if (s === "failed" || s === "closed") {
        this.fail(new Error("The connection to the mixer was lost."), gen);
      } else if (s === "disconnected") {
        // After a change of network the old path is gone for good: offer
        // again now. Otherwise it is a blip, given a while to come back.
        const wait = Date.now() - this.moved < MOVED_MS ? 0 : GRACE_MS;
        this.grace = setTimeout(() => this.fail(new Error("The connection to the mixer dropped and did not come back."), gen), wait);
      }
    };
    pc.addEventListener("connectionstatechange", check);
    check();
  }

  fail(error, gen) {
    if (this.stopped || gen !== this.gen) return;
    this.drop();
    this.error = (error && error.message) || String(error);
    if (isFinal(error)) {
      this.stopped = true;
      this.set("stopped");
      return;
    }
    this.retryIn = backoff(this.failures);
    this.failures += 1;
    this.set("reconnecting");
    this.retry = setTimeout(() => this.attempt(), this.retryIn);
  }

  drop() {
    const live = this.live;
    this.live = null;
    if (!live) return;
    live.pc.close();
    this.end(live.location);
  }

  clearTimers() {
    clearTimeout(this.retry);
    clearTimeout(this.grace);
  }

  set(state) {
    this.state = state;
    if (this.opts.onChange) this.opts.onChange({ state, error: this.error, retryIn: this.retryIn });
  }

  /** The live peer connection, which the stats line reads. */
  get pc() {
    return this.live ? this.live.pc : null;
  }
}
