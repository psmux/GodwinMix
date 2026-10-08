// A tile's level, always on screen, and the face of a source with no picture.
//
// The level used to live in the strip that shows on hover, at 14 by 16 pixels,
// so a person looking at the tray saw no level at all for any input. A
// microphone was worse off: its picture was the mosaic's black cell, which
// reads as a broken camera. Its tile now says it is sound only, and its level
// is the widest thing on it.
//
// The bar sits under the picture and over the name, in the tile's own flow, so
// it covers nothing the camera sees and it is there in every gallery mode.
// "Show meters on tiles" turned off means no bar and no ask: nothing is
// measured for a tile nobody chose to meter.

import { el, svg } from "../../shell/dom.js";
import { ICONS } from "../../client/kinds.js";
import { addView, meterElement } from "../../shell/meter.js";

/**
 * True for a source whose tile should say "sound only" rather than show a
 * picture: no video, and either sound already arriving or a microphone that
 * has not sent any yet. A camera still connecting has neither and keeps its
 * picture box.
 */
export function soundOnly(source, kind) {
  if (source.has_video !== false) return false;
  return source.has_audio === true || kind === "mic";
}

/** Whether this tile gets a level at all. */
export function metered(source, kind, meters) {
  if (meters === false) return false;
  return source.has_audio !== false || soundOnly(source, kind);
}

/**
 * The parts to add to a tile: the bar, the readout it prints the loudest
 * channel into, and for a sound only source the face that stands in for the
 * picture. Any of them may be null.
 */
export function levelParts(source, kind, meters) {
  const sound = soundOnly(source, kind);
  const meter = metered(source, kind, meters) ? meterElement("h") : null;
  if (meter) {
    meter.classList.add("level");
    meter.title = "Input level";
  }
  if (!sound) return { sound, meter, readout: null, face: null, note: null };
  const readout = meter ? el("span.num.sm", { "aria-label": "Peak level in dB" }) : null;
  const note = el("span.sm.dim", { text: noteFor(source) });
  const face = el("div.soundface", {}, [svg(ICONS.mic), el("div.row", {}, [note, readout])]);
  return { sound, meter, readout, face, note };
}

function noteFor(source) {
  return source.has_audio === true ? "Sound only, no video" : "Sound only, nothing heard yet";
}

/** Keep the face's words true as sound starts and stops arriving. */
export function syncLevel(tile, source) {
  if (!tile.note) return;
  const text = noteFor(source);
  if (tile.note.textContent !== text) tile.note.textContent = text;
}

/** Start painting a tile's level. Asking the core is the meter module's job. */
export function watchLevel(tile) {
  if (tile.meter) addView("tile:" + tile.id, "src:" + tile.id, tile.meter, "h", tile.readout || undefined);
}
