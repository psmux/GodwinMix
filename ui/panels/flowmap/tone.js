// The colour word every part of the map shares: ok, busy, warn, bad or
// off, from whatever word a station uses for a state.

const TONE = {
  live: "ok", ok: "ok", running: "ok",
  starting: "busy", connecting: "busy", waiting: "busy",
  silent: "warn", frozen: "warn", scrambled: "warn", reconnecting: "warn", stalled: "warn", warn: "warn",
  nopicture: "bad", black: "bad", failed: "bad", bad: "bad", dead: "bad",
  offline: "off", stopped: "off", off: "off", idle: "off",
};

/** The colour word for a state word. */
export const toneOf = (state) => TONE[state] || "off";
