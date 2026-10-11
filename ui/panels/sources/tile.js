// One tile in the tray.
//
// Built once and then written into. A tile is never rebuilt while a pointer is
// down on one of its controls, and its picture element is chosen by the gallery
// mode rather than the tile being recreated when the mode changes, so a mode
// toggle never ends a gesture. The fader, M and a clip's lane are
// tile-strip.js.

import { el, svg } from "../../shell/dom.js";
import { ICONS, KIND_COLOUR, kindOfSource } from "../../client/kind-look.js";
import { levelParts, syncLevel } from "./tile-level.js";
import { nameOf, colourOf, isLocal } from "./local.js";
import { dragHandle } from "../../shell/pointer.js";
import { playbackText } from "./clip-end.js";
import { lazyLine } from "./backup-slot.js";
import { buildStrip, syncStrip } from "./tile-strip.js";

/**
 * @param {object} source   a SourceStatus
 * @param {object} deps     {audio, scrub, onTake, onRename, onGear, onMute, onRepeat, onBackup, meters}
 */
export function buildTile(source, deps) {
  const kind = kindOfSource(source);
  const node = el("div.tile", { "data-id": source.id, "data-drop": source.id, tabindex: "0", role: "button" });
  node.style.setProperty("--tile-color", colourOf(source, KIND_COLOUR[kind] || "var(--kind-other)"));

  const pic = el("canvas.pic", { width: 320, height: 180 });
  const still = el("img.pic", { alt: "", hidden: true, loading: "lazy" });
  const kindbox = el("div.kindbox", { hidden: true }, [svg(ICONS[kind] || ICONS.stream)]);
  const slot = el("span.badge", { hidden: true });
  const strip = el("div.strip");

  const name = el("span.name.grow.ellipsis", { text: nameOf(source) });
  const dot = el("span.dot");
  const gear = el("button.gear", { text: "⚙", title: "Settings", "aria-label": "Settings", "data-nodrag": "" });
  const bar = el("div.bar", {}, [dragHandle(), dot, name, gear]);

  const playback = el("div.playback", { text: playbackText(source) });
  const level = levelParts(source, kind, deps.meters);
  node.classList.toggle("sound", level.sound);
  node.classList.toggle("metered", !!level.meter);
  // What its backup is doing, or the offer of one when it went down with none.
  const backup = lazyLine({ call: deps.onBackup, openBackup: (id) => deps.onGear(id, "backup"), hint: true });
  node.append(pic, still, kindbox, ...[level.face, level.meter].filter(Boolean), slot, strip, bar, playback, backup.node);
  const controls = buildStrip(source, node, strip, deps);

  gear.onclick = (e) => {
    e.stopPropagation();
    deps.onGear(source.id);
  };

  return {
    ...controls,
    id: source.id,
    node,
    pic,
    still,
    kindbox,
    slot,
    strip,
    name,
    dot,
    meter: level.meter,
    readout: level.readout,
    note: level.note,
    sound: level.sound,
    kind,
    playback,
    backup,
  };
}

/** Write the changing parts into an existing tile. Never rebuilds anything. */
export function syncTile(tile, source, view) {
  const displayName = nameOf(source);
  if (tile.name.textContent !== displayName && !tile.name.isContentEditable) tile.name.textContent = displayName;
  tile.node.title = `${displayName}: ${source.uri}${isLocal(source.id) ? "\n(name and colour are kept on this device only: this mixer has no source.set yet)" : ""}`;
  tile.node.style.setProperty("--tile-color", colourOf(source, KIND_COLOUR[tile.kind] || "var(--kind-other)"));
  tile.dot.className = "dot " + (source.state || "");
  tile.dot.title = source.state || "";

  tile.node.classList.toggle("program", view.tally === "program");
  tile.node.classList.toggle("armed", view.tally === "preview");
  tile.node.classList.toggle("selected", view.selected);

  if (view.slot) {
    tile.slot.hidden = false;
    tile.slot.textContent = String(view.slot);
  } else {
    tile.slot.hidden = true;
  }

  syncStrip(tile, source, view);
  syncLevel(tile, source);
  const playing = playbackText(source);
  if (tile.playback.textContent !== playing) tile.playback.textContent = playing;
  tile.backup.sync(source, view.names || ((id) => id));
}

/** Show the right picture for the gallery mode. Costs nothing to switch. */
export function setTileMode(tile, mode) {
  // A sound only source has no picture in any mode: its face says so.
  tile.pic.hidden = mode !== "live" || tile.sound;
  tile.still.hidden = mode !== "snapshot" || tile.sound;
  tile.kindbox.hidden = mode === "live" || mode === "snapshot" || tile.sound;
  tile.node.classList.toggle("mode-label", mode === "label");
  if (mode === "label") tile.kindbox.hidden = true;
}
