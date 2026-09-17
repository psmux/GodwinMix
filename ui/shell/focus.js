// Which scene the operator is working on.
//
// Not the one on air and not the one armed: an explicit choice, so that a
// take does not pull the source list out from under the operator's hand mid
// show. Seeded by whoever asks first, from armed, then live, then the first
// scene, and remembered on this device.
//
// Modelled on settings.js: one value, a listener set, localStorage behind a
// try, and nothing imported so a test can reach it bare.

const KEY = "gmx.scene.focus";
let focused = null;
let loaded = false;
const listeners = new Set();

function load() {
  if (loaded) return;
  loaded = true;
  try {
    focused = localStorage.getItem(KEY) || null;
  } catch {
    focused = null;
  }
}

/**
 * The focused scene id, or null. With `valid`, an array of scene ids that
 * exist right now, a remembered id that no longer exists reads as null
 * rather than sticking.
 */
export function focusedScene(valid) {
  load();
  if (Array.isArray(valid) && focused !== null && !valid.includes(focused)) return null;
  return focused;
}

export function setFocusedScene(id) {
  load();
  const next = id || null;
  if (next === focused) return;
  focused = next;
  try {
    if (next === null) localStorage.removeItem(KEY);
    else localStorage.setItem(KEY, next);
  } catch {
    /* a private window keeps it for the session only */
  }
  for (const fn of listeners) fn(focused);
}

/** Called with the new id on every change. Returns the unsubscribe. */
export function onFocusChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}
