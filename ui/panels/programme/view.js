// The programme alone, filling whatever it is put in: no toolbar, no label,
// black bars where the display's shape is not the programme's, and no cursor
// once the mouse has been still for a moment.
//
// Shared by the full screen view in the operator's window (fullscreen.js)
// and the output page on another display (ui/programme/). Both ask the core
// for `ext.programme`, the programme at full quality, at the display's width
// in device pixels: a 1080p programme on a 1080p projector is drawn pixel for
// pixel, and on a 4K display it is the 1080p programme the browser scales up
// once. The ask is given back when the view goes, which takes the feed off
// the core (nothing runs unless asked).

import { el, on } from "../../shell/dom.js";

/** How long the mouse has to be still before the cursor and the hint go. */
export const IDLE_MS = 2000;

let styled = false;

function style() {
  if (styled) return;
  styled = true;
  document.head.appendChild(el("link", { rel: "stylesheet", href: new URL("./programme.css", import.meta.url).href }));
}

/**
 * The width to ask the core for: the display's, in device pixels, even, and
 * inside what the protocol takes. The core never scales up past the canvas.
 */
export function displayWidth(scr, dpr) {
  const ratio = dpr || 1;
  const width = Math.round(((scr && scr.width) || 1920) * ratio);
  return Math.max(320, Math.min(7680, width)) & ~1;
}

/**
 * Hide the cursor and the hint after `ms` of no movement. Returns the
 * release function. `now` and `timers` are for the tests.
 */
export function idleCursor(root, ms = IDLE_MS, timers = { set: setTimeout, clear: clearTimeout }) {
  let timer = null;
  const wake = () => {
    root.classList.remove("idle");
    timers.clear(timer);
    timer = timers.set(() => root.classList.add("idle"), ms);
  };
  const offs = ["pointermove", "pointerdown", "keydown"].map((type) => on(root.ownerDocument || document, type, wake));
  wake();
  return () => {
    timers.clear(timer);
    offs.forEach((off) => off());
  };
}

/**
 * Build the view. `hint` is the line shown while the mouse moves; `hintAtStart`
 * shows it once when the view opens, which the operator's own window wants
 * and a projector the audience is looking at does not.
 */
export function programmeView(client, { hint = "", hintAtStart = true } = {}) {
  style();
  const canvas = el("canvas.programme-picture", { width: 16, height: 9, "aria-label": "Programme" });
  canvas.dataset.natural = "";
  const note = el("div.programme-hint", { text: hint, hidden: !hint });
  const root = el("div.programme-full", {}, [canvas, note]);
  if (!hintAtStart) root.classList.add("idle");
  const want = client.want("programme", { width: displayWidth(window.screen, window.devicePixelRatio), fps: 0 });
  const detach = client.programme.attach(canvas);
  const offIdle = hintAtStart ? idleCursor(root) : later(root);
  return {
    root,
    canvas,
    release() {
      detach();
      want.release();
      offIdle();
    },
  };
}

/** Start hiding the cursor only once the mouse first moves. */
function later(root) {
  let off = null;
  const first = on(root.ownerDocument || document, "pointermove", () => {
    first();
    off = idleCursor(root);
  });
  return () => (off ? off() : first());
}
