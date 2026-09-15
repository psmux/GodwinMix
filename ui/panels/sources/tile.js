// One tile in the tray.
//
// Built once and then written into. A tile is never rebuilt while a pointer is
// down on one of its controls, and its picture element is chosen by the gallery
// mode rather than the tile being recreated when the mode changes, so a mode
// toggle never ends a gesture.

import { el, svg } from "../../shell/dom.js";
import { ICONS, KIND_COLOUR, kindOfUri } from "../../client/kinds.js";
import { meterElement } from "../../shell/meter.js";
import { UNITY, gainToPos, gainLabel, fmtPosition } from "../../shell/fader.js";
import { nameOf, colourOf, isLocal } from "./local.js";

/**
 * @param {object} source   a SourceStatus
 * @param {object} deps     {audio, scrub, onTake, onRename, onGear, onMute}
 */
export function buildTile(source, deps) {
  const kind = kindOfUri(source.uri);
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
  const bar = el("div.bar", {}, [dot, name, gear]);

  const playback = el("div.playback", { text: source.seekable ? "Clip" : "Continuous live source" });
  node.append(pic, still, kindbox, slot, strip, bar, playback);

  // ----------------------------------------------------------- the strip

  const mute = el("button.btn.icon", { text: "M", title: "Mute", "data-nodrag": "", disabled: source.has_audio === false });
  const fader = el("input", {
    type: "range",
    min: "0",
    max: "1",
    step: "0.005",
    value: String(gainToPos(source.gain === undefined ? 1 : source.gain)),
    title: "Level",
    "data-nodrag": "",
    disabled: source.has_audio === false,
  });
  const gv = el("span.num.sm", { text: gainLabel(source.gain === undefined ? 1 : source.gain) });
  const meter = meterElement("v");
  strip.append(meter, fader, gv, mute);

  let lane = null;
  let pos = null;
  if (source.seekable) {
    pos = el("span.num.sm.dim", { style: { minWidth: "3.4em" } });
    lane = el("input", { type: "range", min: "0", max: "1000", step: "1", value: "0", title: "Position", "data-nodrag": "" });
    const laneRow = el("div.strip", { style: { bottom: "0", opacity: "1", background: "transparent" } }, [lane, pos]);
    node.appendChild(laneRow);
  }

  gear.onclick = (e) => {
    e.stopPropagation();
    deps.onGear(source.id);
  };
  mute.onclick = (e) => {
    e.stopPropagation();
    deps.onMute(source.id, !mute.classList.contains("on"));
  };
  deps.audio.bindFader(fader, source.id, "gain");
  if (lane) deps.scrub.bind(lane, source.id);

  return {
    id: source.id,
    node,
    pic,
    still,
    kindbox,
    slot,
    strip,
    name,
    dot,
    gv,
    fader,
    mute,
    meter,
    lane,
    pos,
    kind,
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

  const gain = view.gain === undefined ? (source.gain === undefined ? 1 : source.gain) : view.gain;
  if (document.activeElement !== tile.fader && !view.faderBusy) {
    const wanted = String(gainToPos(gain));
    if (tile.fader.value !== wanted) tile.fader.value = wanted;
  }
  tile.gv.textContent = gainLabel(gain);
  tile.gv.classList.toggle("muted", !!source.muted);
  tile.mute.classList.toggle("on", !!source.muted);
  tile.mute.setAttribute("aria-pressed", String(!!source.muted));
  tile.mute.title = source.muted ? "Unmute" : "Mute";

  tile.strip.hidden = !view.showStrip;

  if (tile.lane) {
    const p = view.position || { pos: 0, dur: null };
    if (p.dur) {
      tile.lane.disabled = false;
      if (!view.scrubBusy) tile.lane.value = String(Math.round((p.pos / p.dur) * 1000));
      tile.pos.textContent = `${fmtPosition(p.pos)} / ${fmtPosition(p.dur)}`;
    } else {
      tile.lane.disabled = true;
      tile.pos.textContent = fmtPosition(p.pos);
    }
  }
}

/** Show the right picture for the gallery mode. Costs nothing to switch. */
export function setTileMode(tile, mode) {
  tile.pic.hidden = mode !== "live";
  tile.still.hidden = mode !== "snapshot";
  tile.kindbox.hidden = mode === "live" || mode === "snapshot";
  tile.node.classList.toggle("mode-label", mode === "label");
  if (mode === "label") tile.kindbox.hidden = true;
}
