// Settings: the store every panel reads. The dialog that changes it is
// settings-dialog.js, fetched when it is opened.
//
// Two tabs, simple first. Nothing a volunteer does not need is on the first
// tab. The drawer is a different thing: it is the selected item's own form,
// rendered from its schema, in the `sidebar` slot, so a plugin can add a
// section to its own item's drawer and to nothing else.

const KEY = "gmx.settings";

export const GALLERY_MODES = [
  ["live", "Live", "The picture, moving. Costs the multiview stream."],
  ["snapshot", "Snapshot", "A still, refreshed when you click it or on a timer."],
  ["icon", "Icon", "A coloured button with the kind's icon. Costs nothing."],
  ["label", "Label", "Colour and name only. Costs nothing."],
];

const DEFAULTS = {
  gallery: "live",
  snapshotSecs: 0,
  producer: false,
  meters: true,
  faders: true,
  lanes: true,
  tileWidth: 168,
  multiviewFps: 8,
  confirmRemove: true,
  confirmTake: true,
};

let state = null;
const listeners = new Set();

export function settings() {
  if (state) return state;
  let saved = {};
  try {
    saved = JSON.parse(localStorage.getItem(KEY) || "{}");
  } catch {
    saved = {};
  }
  state = Object.assign({}, DEFAULTS, saved);
  return state;
}

export function setSetting(key, value) {
  settings()[key] = value;
  try {
    localStorage.setItem(KEY, JSON.stringify(state));
  } catch {
    /* the choice lasts the session */
  }
  for (const fn of listeners) fn(state, key);
}

export function onSettingsChanged(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/**
 * The machine proposes the default once: a Pi class machine starts on icons, a
 * laptop on battery on snapshots, everything else live. `gmx doctor` will do
 * this properly from the core side; until then `hardwareConcurrency` and the
 * battery API are what a browser can see, and they are only used to choose a
 * first value the operator can change.
 */
export function proposeGalleryMode() {
  try {
    if (localStorage.getItem(KEY)) return null;
  } catch {
    /* fall through and propose */
  }
  const cores = navigator.hardwareConcurrency || 4;
  if (cores <= 4) return "icon";
  return "live";
}

// ---------------------------------------------------------------- the modal

/** The dialog, in settings-dialog.js, fetched the first time it is opened. */
export function openSettings(client, opts = {}) {
  return import("./settings-dialog.js").then((m) => m.openSettings(client, opts));
}
