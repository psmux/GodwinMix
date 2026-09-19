// Peak meters: the scale, the ballistics and one paint loop for all of them.
// Carried over from the single page UI unchanged, because it was right.
//
// The scale is piecewise so the working range, -20 to 0 dBFS, gets about two
// thirds of the travel. Decay is 60 dB a second, the peak hold sits for 1.1 s
// then slides at 20 dB a second, and a level older than 1.2 s counts as
// silence, so a removed source drains rather than freezing lit.
//
// One requestAnimationFrame loop paints every meter. rAF and not a timer,
// because a hidden tab suspends rAF and a mixer left open on a second monitor
// should cost nothing.

const STOPS = [
  [-60, 0],
  [-40, 0.12],
  [-20, 0.35],
  [-10, 0.6],
  [-6, 0.74],
  [-3, 0.86],
  [0, 1],
];
export const FLOOR = -60;
const FALL_DB_PER_S = 60;
const HOLD_MS = 1100;
const HOLD_FALL_DB_PER_S = 20;
const STALE_MS = 1200;
const EPSILON = 0.0015;

/** dBFS to a fraction of the meter's travel. */
export function dbToPos(db) {
  if (!Number.isFinite(db) || db <= STOPS[0][0]) return 0;
  if (db >= 0) return 1;
  for (let i = 1; i < STOPS.length; i += 1) {
    const [d1, p1] = STOPS[i];
    const [d0, p0] = STOPS[i - 1];
    if (db <= d1) return p0 + ((db - d0) / (d1 - d0)) * (p1 - p0);
  }
  return 1;
}

/** key ("program" or "src:<id>") -> {peaks, t} */
const levels = new Map();
/** view id -> {key, node, orient, bars, held, shown, readout} */
const views = new Map();

export function takeLevel(key, peaks) {
  if (!Array.isArray(peaks)) return;
  // Drop levels nothing is watching, or the map grows by one entry per source
  // the mixer has ever had.
  let watched = false;
  for (const v of views.values()) {
    if (v.key === key) {
      watched = true;
      break;
    }
  }
  if (!watched) return;
  levels.set(key, { peaks, t: performance.now() });
}

/** Feed a whole `event/meters` payload in one call. */
export function takeMeters(params) {
  if (params.program) takeLevel("program", params.program);
  if (params.sources) {
    for (const [id, peaks] of Object.entries(params.sources)) takeLevel("src:" + id, peaks);
  }
}

/**
 * @param {string} viewId  where it is drawn, e.g. "cell:3"
 * @param {string} key     what it draws, e.g. "src:cam1"
 * @param {HTMLElement} node  a .meter element
 * @param {"v"|"h"} orient
 * @param {HTMLElement} [readout]  an element to print the loudest channel into
 */
export function addView(viewId, key, node, orient, readout) {
  views.set(viewId, { key, node, orient: orient || "v", bars: [], held: [], shown: [], readout, channels: 0 });
  start();
}

export function dropView(viewId) {
  const view = views.get(viewId);
  views.delete(viewId);
  if (view && ![...views.values()].some(other => other.key === view.key)) levels.delete(view.key);
}

/** Remove every view whose id starts with the prefix. Used on a cell rebuild. */
export function dropViews(prefix, keep) {
  for (const id of [...views.keys()]) {
    if (id.startsWith(prefix) && !(keep && keep.has(id))) dropView(id);
  }
}

function ensureBars(view, count) {
  if (view.channels === count) return;
  view.channels = count;
  view.node.textContent = "";
  view.bars = [];
  view.held = [];
  view.shown = [];
  for (let i = 0; i < count; i += 1) {
    const ch = document.createElement("div");
    ch.className = "ch";
    const fill = document.createElement("div");
    fill.className = "fill";
    ch.appendChild(fill);
    view.node.appendChild(ch);
    view.bars.push(fill);
    view.held.push({ db: FLOOR, until: 0 });
    view.shown.push(-1);
  }
}

let running = false;

function start() {
  if (running) return;
  running = true;
  requestAnimationFrame(paint);
}

function paint(now) {
  if (views.size === 0) {
    running = false;
    return;
  }
  for (const view of views.values()) {
    const level = levels.get(view.key);
    const fresh = level && now - level.t < STALE_MS;
    const peaks = fresh ? level.peaks : [];
    const count = peaks.length || view.channels || 2;
    ensureBars(view, count);

    let loudest = FLOOR;
    for (let i = 0; i < view.bars.length; i += 1) {
      const target = fresh && Number.isFinite(peaks[i]) ? peaks[i] : FLOOR;
      const hold = view.held[i];
      if (target >= hold.db) {
        hold.db = target;
        hold.until = now + HOLD_MS;
      } else if (now > hold.until) {
        hold.db = Math.max(target, hold.db - (HOLD_FALL_DB_PER_S * 16.7) / 1000);
      }
      // The fill falls at a fixed rate rather than jumping, which is what makes
      // a meter readable rather than a strobe.
      const previous = view.shown[i] < 0 ? 0 : view.shown[i];
      const wanted = dbToPos(target);
      const fallen = Math.max(wanted, previous - (FALL_DB_PER_S * 16.7) / 1000 / 60);
      const pos = wanted >= previous ? wanted : fallen;
      if (Math.abs(pos - previous) > EPSILON || view.shown[i] < 0) {
        view.shown[i] = pos;
        const pct = (1 - pos) * 100;
        // clip-path and not height: no layout pass, and the compositor does it.
        view.bars[i].style.clipPath =
          view.orient === "h" ? `inset(0 ${pct}% 0 0)` : `inset(${pct}% 0 0 0)`;
      }
      if (hold.db > loudest) loudest = hold.db;
    }
    if (view.readout) {
      const text = loudest <= FLOOR ? "" : loudest.toFixed(1);
      if (view.readout.textContent !== text) view.readout.textContent = text;
    }
  }
  requestAnimationFrame(paint);
}

/** Build the bars element a view paints into. */
export function meterElement(orient) {
  const node = document.createElement("div");
  node.className = "meter" + (orient === "h" ? " h" : "");
  return node;
}
