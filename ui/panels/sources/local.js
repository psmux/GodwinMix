// Names and colours the server cannot keep yet.
//
// `source.set {name, color}` is the right home for both: stored on the
// document, the same in the terminal UI, the Stream Deck, the tally and the
// agent's view. Today's core has no such method, so a rename made here is kept
// on this device and marked as local, and the tile says so in its tooltip. The
// moment the core answers `source.set` the local copy is dropped and the
// server's value wins, which is why `adopt` exists.

const KEY = "gmx.tiles";

let cache = null;

function read() {
  if (cache) return cache;
  try {
    cache = JSON.parse(localStorage.getItem(KEY) || "{}");
  } catch {
    cache = {};
  }
  return cache;
}

function write() {
  try {
    localStorage.setItem(KEY, JSON.stringify(cache));
  } catch {
    /* the change lasts the session */
  }
}

export function localOf(id) {
  return read()[id] || null;
}

export function setLocal(id, fields) {
  const all = read();
  all[id] = Object.assign({}, all[id], fields);
  write();
}

export function clearLocal(id) {
  const all = read();
  delete all[id];
  write();
}

/** The server started keeping this itself: forget our copy. */
export function adopt(id) {
  clearLocal(id);
}

/** The name to show: the local one if there is one, otherwise the server's. */
export function nameOf(source) {
  const local = localOf(source.id);
  return (local && local.name) || source.name || source.id;
}

export function colourOf(source, kindColour) {
  const local = localOf(source.id);
  return (local && local.color) || source.color || kindColour;
}

export function isLocal(id) {
  const local = localOf(id);
  return !!(local && (local.name || local.color));
}
