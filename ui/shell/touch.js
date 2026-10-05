// A finger's way to the right click and the double click.
//
// Every item menu in the page listens for `contextmenu` and every "open this"
// listens for `dblclick`. A phone has neither: Safari on iOS never fires a
// contextmenu for a touch, Chrome on Android fires one only sometimes, and
// whether a double tap becomes a dblclick depends on the zoom settings. So
// rather than teach each panel about touch, this module turns a long press
// into a `contextmenu` and a double tap into a `dblclick`, dispatched on the
// element the finger was on, and swallows the ones the browser makes itself
// after a touch so nothing opens twice. Every listener already written keeps
// working unchanged.
//
// Text fields are left alone. A long press there is the system's, for
// selecting and pasting. So is anything under `[data-drag-handle]`, where a
// held finger is the start of a drag rather than a request for a menu.

/** How long a finger rests before it counts as a long press. */
export const LONG_PRESS_MS = 500;
/** Travel in CSS pixels that turns a press into a scroll and cancels it. */
export const SLOP = 8;
/** The gap and the distance within which a second tap is a double tap. */
export const DOUBLE_TAP_MS = 320;
export const DOUBLE_TAP_PX = 24;
/** How long after a touch a browser's own menu or dblclick is ours to drop. */
const ECHO_MS = 800;

// A fader is an input too, but a double tap on one resets it to unity the way
// a double click does, so ranges and boxes are not counted as text.
const EDITABLE = "input:not([type=range]):not([type=checkbox]):not([type=radio]), textarea, select, [contenteditable]:not([contenteditable=false])";

/** True for a pointer that is a finger or a pen rather than a mouse. */
export const isTouch = (e) => e.pointerType === "touch" || e.pointerType === "pen";

/** Words for the hand in use: "Double click" to a mouse, "Double tap" to a finger. */
export function byPointer(mouse, finger) {
  return typeof matchMedia === "function" && matchMedia("(pointer: coarse)").matches ? finger : mouse;
}

/** Where a long press and a double tap mean nothing of ours. */
export function ignored(target) {
  return !target || !target.closest || !!target.closest(`${EDITABLE}, [data-drag-handle]`);
}

/** Where the browser's own long press menu is wanted: text, to select and paste. */
const editable = (target) => !target || !target.closest || !!target.closest(EDITABLE);

/**
 * Watch one root for long presses and double taps. Returns the function that
 * stops watching. `now` is injectable for the tests; the page uses the clock.
 */
export function watchTouch(root, opts = {}) {
  const now = opts.now || (() => performance.now());
  const ms = opts.longPress ?? LONG_PRESS_MS;
  let press = null;
  let lastTap = null;
  let lastTouch = -Infinity;
  let firedAt = -Infinity;

  const cancel = () => {
    if (press) clearTimeout(press.timer);
    press = null;
  };
  const fire = () => {
    const p = press;
    press = null;
    if (!p || !p.target.isConnected) return;
    lastTap = null;
    // Only a press that opened a menu eats the click that follows. A Take
    // button held down a moment too long must still take.
    const opened = !p.target.dispatchEvent(mouse("contextmenu", p, 2));
    if (opened) firedAt = now();
  };

  const down = (e) => {
    if (!isTouch(e) || !e.isPrimary) return;
    lastTouch = now();
    cancel();
    if (ignored(e.target)) return;
    press = { target: e.target, x: e.clientX, y: e.clientY, pointerId: e.pointerId };
    press.timer = setTimeout(fire, ms);
  };
  const move = (e) => {
    if (!press || e.pointerId !== press.pointerId) return;
    if (Math.hypot(e.clientX - press.x, e.clientY - press.y) > SLOP) cancel();
  };
  const up = (e) => {
    if (!isTouch(e)) return;
    lastTouch = now();
    // The target the finger went down on: a panel that captured the pointer
    // gets the up retargeted to itself, which is not what was tapped.
    const target = press && e.pointerId === press.pointerId ? press.target : null;
    cancel();
    if (!target) return;
    const t = now();
    const tap = { target, x: e.clientX, y: e.clientY, t };
    if (lastTap && t - lastTap.t <= DOUBLE_TAP_MS && Math.hypot(tap.x - lastTap.x, tap.y - lastTap.y) <= DOUBLE_TAP_PX) {
      lastTap = null;
      // After the click this tap makes, the way a mouse orders them.
      setTimeout(() => tap.target.isConnected && tap.target.dispatchEvent(mouse("dblclick", tap, 0)), 0);
      return;
    }
    lastTap = tap;
  };
  // The browser's own, after a touch: ours already went, or is on its way. On
  // the grip there is none of ours, and a menu there would interrupt a drag.
  const echo = (e) => {
    if (!e.isTrusted || now() - lastTouch > ECHO_MS || editable(e.target)) return;
    e.preventDefault();
    e.stopImmediatePropagation();
  };
  // The click a finger makes when it lifts after a long press is not a tap.
  const click = (e) => {
    if (now() - firedAt > ECHO_MS) return;
    firedAt = -Infinity;
    e.preventDefault();
    e.stopImmediatePropagation();
  };

  const offs = [
    ["pointerdown", down], ["pointermove", move], ["pointerup", up], ["pointercancel", cancel],
    ["contextmenu", echo], ["dblclick", echo], ["click", click],
  ].map(([type, fn]) => {
    root.addEventListener(type, fn, true);
    return () => root.removeEventListener(type, fn, true);
  });
  return () => {
    cancel();
    for (const off of offs) off();
  };
}

let installed = null;

/** Once per page, on the document. */
export function installTouch() {
  installed ||= watchTouch(document);
  return installed;
}

function mouse(type, at, button) {
  return new MouseEvent(type, {
    bubbles: true, cancelable: true, composed: true, view: window,
    clientX: at.x, clientY: at.y, button, buttons: 0, detail: type === "dblclick" ? 2 : 1,
  });
}
