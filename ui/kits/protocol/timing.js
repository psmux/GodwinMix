// How long a drag takes to draw, and how long the core takes to agree.
//
// The two numbers 07 Phase 3 asks for: a drag redraws locally within 16 ms and
// the core's echo arrives under 25 ms on the same host. They are measured here
// rather than guessed, and the composer prints them in the test page.
//
// A ring of samples, no allocation per sample after the first pass, because a
// drag produces one of these per pointer move and the measurement must not be
// the thing that costs the frame.

const KEEP = 240;

export class Timings {
  constructor(keep = KEEP) {
    this.keep = keep;
    this.rows = new Map();
  }

  /** One sample, in milliseconds. */
  note(name, ms) {
    let row = this.rows.get(name);
    if (!row) {
      row = { at: 0, n: 0, values: new Array(this.keep).fill(0) };
      this.rows.set(name, row);
    }
    row.values[row.at] = ms;
    row.at = (row.at + 1) % this.keep;
    row.n += 1;
    return ms;
  }

  /** Time a function and file the result under `name`. */
  measure(name, fn) {
    const start = now();
    const out = fn();
    this.note(name, now() - start);
    return out;
  }

  /** `{n, p50, p95, worst}` in milliseconds, or null when nothing was measured. */
  stats(name) {
    const row = this.rows.get(name);
    if (!row || !row.n) return null;
    const taken = Math.min(row.n, this.keep);
    const sorted = row.values.slice(0, taken).sort((a, b) => a - b);
    return {
      n: row.n,
      p50: sorted[Math.floor(taken * 0.5)],
      p95: sorted[Math.min(taken - 1, Math.floor(taken * 0.95))],
      worst: sorted[taken - 1],
    };
  }

  /** One line per measurement, for a log or a test page. */
  lines() {
    const out = [];
    for (const name of this.rows.keys()) {
      const s = this.stats(name);
      if (s) out.push(`${name}: ${s.n} samples, p50 ${ms(s.p50)}, p95 ${ms(s.p95)}, worst ${ms(s.worst)}`);
    }
    return out;
  }
}

function ms(v) {
  return `${v.toFixed(2)} ms`;
}

/** The monotonic clock where there is one, the wall clock where there is not. */
export function now() {
  return typeof performance !== "undefined" && performance.now ? performance.now() : Date.now();
}
