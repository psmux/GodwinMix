// The mixer's clock, as this screen reads it.
//
// Every screen works out where the prompter is from a position, the moment
// it was true on the mixer's clock, and a speed. Two screens agree only as
// well as they agree on the time, so each asks the mixer a few times and
// keeps the answer that came back quickest: half of that round trip is the
// most it can be wrong by, which on a LAN is a few milliseconds.

const SAMPLES = 5;

export class MixerClock {
  /** `ask` resolves to something with `now_ms`, the mixer's clock. */
  constructor(ask) {
    this.ask = ask;
    this.offset = 0;
    this.best = Infinity;
  }

  /** Ask a few times and keep the quickest answer. */
  async sync() {
    this.best = Infinity;
    for (let i = 0; i < SAMPLES; i += 1) {
      const sent = Date.now();
      let answer;
      try {
        answer = await this.ask();
      } catch {
        return;
      }
      const back = Date.now();
      const trip = back - sent;
      if (answer && typeof answer.now_ms === "number" && trip < this.best) {
        this.best = trip;
        this.offset = answer.now_ms - (sent + back) / 2;
      }
    }
  }

  /** Milliseconds since the epoch, as the mixer would say. */
  now() {
    return Date.now() + this.offset;
  }
}

/** Where a prompter state says it is at `now`, in rows. */
export function positionAt(state, now) {
  if (!state) return 0;
  const moved = state.playing ? (state.speed * Math.max(0, now - state.at_ms)) / 1000 : 0;
  return Math.min(Math.max(state.position + moved, 0), state.rows || 0);
}
