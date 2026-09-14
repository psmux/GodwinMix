// Faders and scrubbers: the curve, and the tug of war between the operator's
// hand and the server's answer.
//
// The curve is a straight line in dB, which is the only curve that feels right
// under a finger: 0 to 0.75 of the travel is -60 dB to unity, 0.75 to 1.0 is
// unity to +20 dB, which is the mixer's ceiling of 10.0 linear.
//
// The tug of war is the part that took the old page three tries. While a
// pointer is down on a control the server never writes it. After the pointer
// lifts there is a settle window, long enough to cover the debounce plus a
// round trip, in which a status snapshot that still carries the old value is
// ignored. Without it the slider springs back under the hand.

export const UNITY = 0.75;
const BOTTOM_DB = -60;
const TOP_DB = 20;
const SEND_MS = 140;
const SETTLE_MS = 900;
const SEEK_MS = 120;
const SCRUB_HOLD_MS = 3000;
const SCRUB_ANSWER_MS = 1500;

export function posToGain(pos) {
  if (pos <= 0.004) return 0;
  const db = pos <= UNITY ? BOTTOM_DB + (pos / UNITY) * -BOTTOM_DB : ((pos - UNITY) / (1 - UNITY)) * TOP_DB;
  return Math.pow(10, db / 20);
}

export function gainToPos(gain) {
  if (!(gain > 0)) return 0;
  const db = 20 * Math.log10(gain);
  if (db <= BOTTOM_DB) return 0;
  const pos = db <= 0 ? ((db - BOTTOM_DB) / -BOTTOM_DB) * UNITY : UNITY + (db / TOP_DB) * (1 - UNITY);
  return Math.min(1, Math.max(0, pos));
}

export function gainLabel(gain) {
  if (!(gain > 0.0011)) return "off";
  const db = 20 * Math.log10(gain);
  if (Math.abs(db) < 0.05) return "0.0";
  return (db > 0 ? "+" : "") + db.toFixed(1);
}

/** One of these per page. Panels share it so two panels never fight. */
export class AudioGestures {
  constructor(client) {
    this.client = client;
    this.local = new Map();
    this.active = new Set();
    this.timers = new Map();
    this.settle = new Map();
    this.deferred = null;
  }

  /** True while any fader or scrubber is under a pointer. */
  get busy() {
    return this.active.size > 0;
  }

  /** What the control should show: the local value while it is in play. */
  shown(key, serverValue) {
    if (this.active.has(key)) return this.local.get(key);
    const until = this.settle.get(key) || 0;
    if (performance.now() < until && this.local.has(key)) return this.local.get(key);
    return serverValue;
  }

  /**
   * Bind a range input to one audio channel.
   * @param {HTMLInputElement} input
   * @param {string} sourceId
   * @param {"gain"|"page"|`media:${number}`} channel
   */
  bindFader(input, sourceId, channel) {
    const key = sourceId + "/" + channel;
    const send = () => {
      const value = Number(input.value);
      this.local.set(key, value);
      this._post(sourceId, channel, value);
    };
    input.addEventListener("pointerdown", (e) => {
      // The second click of a double click must not jump the level.
      if (e.detail > 1) e.preventDefault();
      this.active.add(key);
    });
    input.addEventListener("input", () => {
      this.local.set(key, Number(input.value));
      this.active.add(key);
      const existing = this.timers.get(key);
      if (existing) clearTimeout(existing);
      this.timers.set(key, setTimeout(send, SEND_MS));
    });
    input.addEventListener("change", () => {
      const existing = this.timers.get(key);
      if (existing) clearTimeout(existing);
      this.timers.delete(key);
      send();
      this._release(key);
    });
    input.addEventListener("dblclick", () => {
      input.value = String(channel === "gain" ? UNITY : 1);
      send();
      this._release(key);
    });
    input.addEventListener("pointercancel", () => this._release(key));
    return key;
  }

  _release(key) {
    this.active.delete(key);
    this.settle.set(key, performance.now() + SETTLE_MS);
    if (this.deferred && !this.busy) {
      const fn = this.deferred;
      this.deferred = null;
      fn();
    }
  }

  /** A rebuild that must wait until the hand comes off. */
  defer(fn) {
    if (!this.busy) {
      fn();
      return;
    }
    this.deferred = fn;
  }

  _post(sourceId, channel, value) {
    const params = { source: sourceId };
    if (channel === "gain") params.gain = posToGain(value);
    else if (channel === "page") params.page = value;
    else if (channel.startsWith("media:")) {
      const index = Number(channel.slice(6));
      // Sparse on purpose: [null, 0.5] moves the second video and nothing else.
      const media = new Array(index + 1).fill(null);
      media[index] = value;
      params.media = media;
    }
    this.client.call("source.audio.set", params).catch((e) => {
      console.warn("the mixer refused a level change", e.message);
    });
  }

  setMuted(sourceId, muted) {
    return this.client.call("source.audio.set", { source: sourceId, muted });
  }
}

/** The scrubber's own hold, which is a different problem from the fader's. */
export class ScrubGestures {
  constructor(client) {
    this.client = client;
    this.positions = new Map();
    this.active = new Set();
    this.hold = new Map();
    this.timers = new Map();
    this.warned = false;
  }

  get busy() {
    return this.active.size > 0;
  }

  take(sourceId, positionMs, durationMs) {
    const held = this.hold.get(sourceId);
    if (held && Math.abs(positionMs - held.ms) < 1000) this.hold.delete(sourceId);
    this.positions.set(sourceId, { pos: positionMs, dur: durationMs, t: performance.now() });
  }

  /** Seed from a status snapshot, but only when nothing newer is known. */
  seed(sourceId, positionMs, durationMs) {
    const known = this.positions.get(sourceId);
    if (known && performance.now() - known.t < SCRUB_ANSWER_MS) return;
    this.positions.set(sourceId, { pos: positionMs, dur: durationMs, t: performance.now() });
  }

  read(sourceId) {
    const held = this.hold.get(sourceId);
    const known = this.positions.get(sourceId) || { pos: 0, dur: null };
    if (held && performance.now() < held.until) return { pos: held.ms, dur: known.dur };
    return known;
  }

  bind(input, sourceId) {
    input.addEventListener("pointerdown", () => this.active.add(sourceId));
    input.addEventListener("input", () => this.active.add(sourceId));
    input.addEventListener("change", () => {
      const known = this.positions.get(sourceId);
      const dur = known && known.dur ? known.dur : 0;
      const ms = (Number(input.value) / 1000) * dur;
      this.active.delete(sourceId);
      this.hold.set(sourceId, { ms, until: performance.now() + SCRUB_HOLD_MS });
      const existing = this.timers.get(sourceId);
      if (existing) clearTimeout(existing);
      this.timers.set(sourceId, setTimeout(() => this._post(sourceId, ms), SEEK_MS));
    });
  }

  async _post(sourceId, ms) {
    try {
      const answer = await this.client.call("source.seek", { source: sourceId, position_ms: ms });
      if (answer && Number.isFinite(answer.position_ms)) {
        this.positions.set(sourceId, { pos: answer.position_ms, dur: answer.duration_ms, t: performance.now() });
        // The answer is the mixer agreeing, not the pipeline having arrived, so
        // the hold is renewed against the answered position rather than dropped.
        this.hold.set(sourceId, { ms: answer.position_ms, until: performance.now() + SCRUB_ANSWER_MS });
      }
    } catch (e) {
      this.hold.delete(sourceId);
      if (!this.warned) {
        this.warned = true;
        console.warn("this mixer did not accept a seek", e.message);
      }
    }
  }
}

export function fmtPosition(ms) {
  if (!Number.isFinite(ms)) return "";
  const s = Math.max(0, Math.round(ms / 1000));
  const m = Math.floor(s / 60);
  const r = s % 60;
  return `${m}:${String(r).padStart(2, "0")}`;
}
