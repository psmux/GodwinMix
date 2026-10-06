// What the next Take does: which transition, which way or which colour, how it
// eases and how long it runs. No elements here, so the tests can reach it
// without a page, and the picker and the quick picks read the same answer.
//
// Remembered in this browser, so the desk's choice is there the next time the
// page opens, and so is a count of how often each one was taken with, which
// is what decides the quick picks beside Take.

const KEY = "gmx.studio.take";
const USED = "gmx.studio.used";

export const TYPES = [
  { type: "fade", label: "Fade" },
  { type: "move", label: "Move" },
  { type: "wipe", label: "Wipe", option: "direction" },
  { type: "slide", label: "Slide", option: "direction" },
  { type: "push", label: "Push", option: "direction" },
  { type: "zoom", label: "Zoom" },
  { type: "zoom-out", label: "Zoom out" },
  { type: "box", label: "Box" },
  { type: "dip", label: "Dip", option: "colour" },
];
export const OPTIONS = {
  direction: [["left", "Left"], ["right", "Right"], ["up", "Up"], ["down", "Down"]],
  colour: [["black", "To black"], ["white", "To white"]],
};
export const EASINGS = [["ease-in-out", "Smooth"], ["linear", "Linear"], ["ease-in", "Ease in"], ["ease-out", "Ease out"]];
export const DURATIONS = [[250, "0.25 s"], [500, "0.5 s"], [1000, "1 s"], [2000, "2 s"]];
/** What the quick picks are before anything has been taken with. */
export const FIRST_PICKS = ["fade", "wipe", "dip"];

function read(key) {
  try {
    return JSON.parse(localStorage.getItem(key)) || {};
  } catch {
    return {};
  }
}

function write(key, value) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    /* the choice lasts the session */
  }
}

/** The chosen transition, with everything a take needs to name it. */
export class TransitionState {
  constructor() {
    const saved = read(KEY);
    this.type = saved.type || "fade";
    this.option = saved.option || "";
    this.easing = saved.easing || "ease-in-out";
    this.length = Number(saved.ms) || 500;
    /** Names the core adds: the collection's, the plugins', the fx library's. */
    this.extra = new Map();
    this.listeners = new Set();
  }

  kind(type = this.type) {
    return TYPES.find((t) => t.type === type) || this.extra.get(type) || { type, label: type };
  }

  /** The directions or the colours this one reads, or none. */
  options(type = this.type) {
    return OPTIONS[this.kind(type).option] || [];
  }

  ms() {
    return this.length;
  }

  /** Change any of type, option, easing or length, and tell whoever listens. */
  set(fields) {
    if (fields.type !== undefined && fields.type !== this.type) {
      this.type = fields.type;
      const pairs = this.options();
      if (!pairs.some(([v]) => v === this.option)) this.option = pairs.length ? pairs[0][0] : "";
    }
    if (fields.option !== undefined) this.option = fields.option;
    if (fields.easing !== undefined) this.easing = fields.easing;
    if (fields.ms !== undefined) this.length = Number(fields.ms) || 500;
    write(KEY, { type: this.type, option: this.option, easing: this.easing, ms: this.length });
    for (const fn of this.listeners) fn(this);
  }

  onChange(fn) {
    this.listeners.add(fn);
    return () => this.listeners.delete(fn);
  }

  /** A short line for the control beside Take: "Wipe left 0.5 s". */
  describe() {
    const k = this.kind();
    const pair = k.option ? this.options().find(([v]) => v === this.option) : null;
    const how = pair ? ` ${pair[1].toLowerCase()}` : "";
    return `${k.label || this.type}${how} ${this.lengthText()}`.trim();
  }

  /** A clip from the fx library runs for its own length, whatever is chosen. */
  lengthText() {
    const k = this.kind();
    if (k.ownMs) return `${k.ownMs / 1000} s`;
    const named = DURATIONS.find(([v]) => v === this.length);
    return named ? named[1] : `${this.length / 1000} s`;
  }

  /** `{type, duration_ms}`, with `params` only when there is something in it. */
  request(durationMs) {
    const k = this.kind();
    const out = { type: this.type, duration_ms: durationMs };
    const params = {};
    if (k.option && this.option) params[k.option] = this.option;
    if (this.easing !== "ease-in-out" && k.origin !== "plugin") params.easing = this.easing;
    if (Object.keys(params).length) out.params = params;
    return out;
  }

  /** Count a take with the current one, for the quick picks. */
  used() {
    const counts = read(USED);
    counts[this.type] = (counts[this.type] || 0) + 1;
    write(USED, counts);
  }

  /** The three most taken with, topped up from FIRST_PICKS, all still known. */
  picks(n = 3) {
    const counts = read(USED);
    const known = (t) => TYPES.some((b) => b.type === t) || this.extra.has(t);
    const ranked = Object.keys(counts).filter(known).sort((a, b) => counts[b] - counts[a]);
    return [...new Set([...ranked, ...FIRST_PICKS])].slice(0, n);
  }

  /** Add the names `program.transitions` lists beyond the built in ones. */
  learn(transitions) {
    for (const t of transitions || []) {
      if (t.origin === "built-in" || TYPES.some((b) => b.type === t.name)) continue;
      const clip = t.origin === "fx" && (t.type === "stinger" || t.type === "overlay");
      this.extra.set(t.name, { type: t.name, label: t.title || t.name, origin: t.origin, ownMs: clip ? t.duration_ms : 0 });
    }
  }
}
