// A publish that stays up: connect, watch the connection, and when it fails
// publish again after a wait that grows, until somebody presses Stop.
//
// The states a person sees: connecting, live, reconnecting, stopped. A
// refusal that waiting will not cure (a wrong key, no such channel, a browser
// with no H.264) stops at once with the mixer's own sentence.

import { publish, unpublish, WhipError } from "./whip.js";

/** Refusals that publishing again cannot fix. 409 (name in use) can. */
const FINAL = new Set([0, 400, 401, 403, 404, 415]);

/** 1, 2, 4, 8 seconds, then every 15. */
export function backoff(failures) {
  return Math.min(15000, 1000 * 2 ** Math.max(0, failures));
}

export function isFinal(error) {
  return error instanceof WhipError && FINAL.has(error.status);
}

/** How long a "disconnected" connection is given to come back by itself. */
const GRACE_MS = 5000;

export class Session {
  /**
   * @param {{url: string, key: string, tracks?: {video?: MediaStreamTrack, audio?: MediaStreamTrack},
   *          onChange?: (s: {state: string, error: string, retryIn: number}) => void,
   *          connect?: Function, end?: Function}} opts
   * `connect` and `end` are the WHIP calls, swappable so the tests need no network.
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

  async attempt() {
    if (this.stopped) return;
    const gen = ++this.gen;
    this.retryIn = 0;
    this.set(this.failures ? "reconnecting" : "connecting");
    let live;
    try {
      live = await this.connect({ url: this.opts.url, key: this.opts.key, media: { ...this.tracks } });
    } catch (e) {
      return this.fail(e, gen);
    }
    if (this.stopped || gen !== this.gen) {
      live.pc.close();
      this.end(live.location);
      return;
    }
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
        this.grace = setTimeout(() => this.fail(new Error("The connection to the mixer dropped and did not come back."), gen), GRACE_MS);
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
    try {
      live.pc.close();
    } catch {
      /* already closed */
    }
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

  /** The live peer connection, for the stats line. */
  get pc() {
    return this.live ? this.live.pc : null;
  }
}
