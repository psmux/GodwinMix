// The frame rate menu in the Preview and Programme title bars, and what a
// choice there changes.
//
// A choice is a cap on that one monitor. Auto is the Picture rate from
// Settings; a number lower than that is what the monitor shows instead. It
// saves work where the work is the monitor's own:
//
// * an armed scene in Preview is its own stream, so the core is asked for it
//   at the capped rate and makes and sends no more than that;
// * a source in Preview, a suggested scene and the Programme picture are cut
//   out of the shared mosaic. The page asks the mosaic for the highest rate
//   the monitors it feeds need, so with both monitors capped and nothing else
//   asking (no live source tiles, no other page), the mosaic itself runs at
//   the cap. When something else wants it faster, the cap only skips paints.
//
// Its own module, so panel.js stays about the programme monitor.

import { el } from "../../shell/dom.js";
import { cellOn } from "../../client/painters.js";
import { settings, setSetting } from "../../shell/settings.js";

export const RATE_CHOICES = [0, 30, 25, 15, 10, 5];
const KEYS = { preview: "previewFpsCap", programme: "programmeFpsCap" };

/** The stored cap for one monitor: 0 for Auto, or one of the choices. */
export function monitorCap(which) {
  const cap = Number(settings()[KEYS[which]]) || 0;
  return RATE_CHOICES.includes(cap) ? cap : 0;
}

/** The rate a monitor shows: the Picture rate, or its cap when that is lower. */
export function monitorFps(which) {
  const rate = settings().multiviewFps;
  const cap = monitorCap(which);
  return cap ? Math.min(cap, rate) : rate;
}

/** Whether a monitor shows fewer frames than the Picture rate. */
export function capped(which) {
  return monitorFps(which) < settings().multiviewFps;
}

/**
 * What the programme monitor asks the mosaic for: the highest rate of the
 * monitors painted from it. Preview counts only while it is drawn from the
 * mosaic (a source's cell or a suggested scene), not while it has its own
 * stream.
 */
export function mosaicRate(panel) {
  const fromMosaic = panel.previewCell !== undefined || !!panel.offDrawn;
  return Math.max(monitorFps("programme"), fromMosaic ? monitorFps("preview") : 0);
}

/** The small menu for one monitor's title bar. */
export function rateMenu(which, label) {
  const select = el("select.monitor-rate", {
    "aria-label": `${label} frame rate`,
    title: `Frames a second on ${label}. Auto is the Picture rate in Settings; a lower number saves work.`,
    onchange: () => setSetting(KEYS[which], Number(select.value)),
    // A double click on Preview takes it; one on its menu must not.
    ondblclick: (e) => e.stopPropagation(),
  }, RATE_CHOICES.map((fps) => el("option", { value: String(fps), text: fps ? `${fps} fps` : "Auto" })));
  select.value = String(monitorCap(which));
  return select;
}

/**
 * Decides whether a frame arriving now is painted, for at most `fps()` a
 * second. A frame up to half a gap early still counts, so a stream already at
 * the cap, with the usual jitter, loses nothing; the schedule moves on by one
 * gap per paint, so the average never goes over.
 */
export function pacer(fps, now = () => performance.now()) {
  let due = -Infinity;
  return () => {
    const t = now();
    const gap = 1000 / Math.max(1, fps());
    if (t < due - gap / 2) return false;
    due = Math.max(due, t - gap / 2) + gap;
    return true;
  };
}

/**
 * Paint one cell of the mosaic onto `canvas`, at the monitor's rate when it is
 * capped and on every sheet when it is not. Returns a release function.
 */
export function attachCell(sheet, canvas, cell, which) {
  if (!capped(which)) return sheet.attach(canvas, cell);
  const due = pacer(() => monitorFps(which));
  if (sheet.bitmap && sheet.layout) drawCell(canvas, sheet.bitmap, sheet.layout, cell);
  return sheet.observe((bitmap, layout) => {
    if (due()) drawCell(canvas, bitmap, layout, cell);
  });
}

/** A paced painter for a picture made of several cells (a suggested scene). */
export function pacedObserver(which, fn) {
  if (!capped(which)) return fn;
  const due = pacer(() => monitorFps(which));
  return (bitmap, layout) => {
    if (due()) fn(bitmap, layout);
  };
}

function drawCell(canvas, bitmap, layout, index) {
  const cell = (layout.cells || []).find((c) => c.index === index);
  if (!cell || !canvas.width || !canvas.height) return;
  const ctx = canvas.getContext("2d", { alpha: false });
  if (ctx) ctx.drawImage(bitmap, ...cellOn(bitmap, layout, cell), 0, 0, canvas.width, canvas.height);
}
