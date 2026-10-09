// A floating card a person can move out of the way by its title bar.
//
// The card keeps its stylesheet place (a corner, or above the phone's tab
// bar) until it is first dragged. From then on it sits where it was put, kept
// inside the window when the window shrinks or a phone turns, and comes back
// there on the next visit. Pointer events, so a finger moves it as a mouse
// does; the bar is `touch-action: none` so the drag is not taken for a scroll.

import { on } from "./dom.js";
import { DRAG_THRESHOLD } from "./pointer.js";

const EDGE = 8;

/**
 * Make `node` movable by `handle`. A press on a button inside the handle is
 * left to the button. `key` names the place in localStorage. Returns the
 * function that stops it.
 */
export function movable(node, handle, key) {
  handle.style.touchAction = "none";
  handle.style.cursor = "grab";
  const saved = load(key);
  if (saved) requestAnimationFrame(() => place(node, saved.x, saved.y));
  let press = null;
  const offs = [
    on(handle, "pointerdown", (e) => {
      if (e.button !== 0 || e.target.closest("button, input, select, a")) return;
      const r = node.getBoundingClientRect();
      press = { id: e.pointerId, x: e.clientX, y: e.clientY, dx: e.clientX - r.left, dy: e.clientY - r.top, moved: false };
    }),
    on(handle, "pointermove", (e) => {
      if (!press || e.pointerId !== press.id) return;
      if (!press.moved && Math.hypot(e.clientX - press.x, e.clientY - press.y) < DRAG_THRESHOLD) return;
      if (!press.moved) {
        press.moved = true;
        handle.setPointerCapture(e.pointerId);
        handle.style.cursor = "grabbing";
      }
      place(node, e.clientX - press.dx, e.clientY - press.dy);
    }),
    on(handle, "pointerup", (e) => end(e, true)),
    on(handle, "pointercancel", (e) => end(e, false)),
    on(window, "resize", () => refit(node)),
  ];
  // Unfolding a card near the foot of the window grows it upward, not off it.
  const watch = new ResizeObserver(() => refit(node));
  watch.observe(node);
  offs.push(() => watch.disconnect());
  function end(e, keep) {
    if (!press || e.pointerId !== press.id) return;
    if (press.moved && keep) store(key, { x: parseFloat(node.style.left), y: parseFloat(node.style.top) });
    // The release that ends a drag is not also a click on the bar.
    // A finger's drag sends no click at all, so the guard goes after a moment
    // rather than waiting to eat the next tap somewhere else.
    if (press.moved) {
      const eat = (c) => c.stopPropagation();
      addEventListener("click", eat, { capture: true, once: true });
      setTimeout(() => removeEventListener("click", eat, { capture: true }), 250);
    }
    handle.style.cursor = "grab";
    press = null;
  }
  return () => offs.forEach((off) => off());
}

/** Put the card's top left corner at x, y, wholly inside the window. */
export function place(node, x, y) {
  const r = node.getBoundingClientRect();
  const left = Math.max(EDGE, Math.min(x, innerWidth - r.width - EDGE));
  const top = Math.max(EDGE, Math.min(y, innerHeight - r.height - EDGE));
  Object.assign(node.style, { left: left + "px", top: top + "px", right: "auto", bottom: "auto" });
}

/** A card that has been moved, kept inside after the window or the card changed size. */
function refit(node) {
  if (node.style.left) place(node, parseFloat(node.style.left), parseFloat(node.style.top));
}

function load(key) {
  try {
    const at = JSON.parse(localStorage.getItem(key) || "null");
    return at && Number.isFinite(at.x) && Number.isFinite(at.y) ? at : null;
  } catch {
    return null;
  }
}

function store(key, at) {
  try {
    localStorage.setItem(key, JSON.stringify(at));
  } catch {
    /* a private window keeps the place for this visit only */
  }
}
