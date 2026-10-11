// A tile's strip: the fader, its level in dB and M, and for a clip the
// position lane with Repeat. Built once with the tile and written into after.

import { el } from "../../shell/dom.js";
import { gainToPos, gainLabel, fmtPosition } from "../../shell/fader.js";
import { repeatButton, syncRepeat } from "./clip-end.js";

/**
 * The strip's controls, appended to `strip` and, for a clip, a lane row
 * appended to `node`.
 *
 * @param {object} source   a SourceStatus
 * @param {object} deps     {audio, scrub, onMute, onRepeat}
 */
export function buildStrip(source, node, strip, deps) {
  // A button that cannot be pressed says why: M and the fader are dead on a
  // source with no sound in it and nothing else on the tile accounts for that.
  const silent = source.has_audio === false;
  const gain = source.gain === undefined ? 1 : source.gain;
  const mute = el("button.btn.icon", {
    text: "M",
    title: silent ? "This source has no audio" : "Mute",
    "data-nodrag": "",
    disabled: silent,
  });
  const fader = el("input", {
    type: "range",
    min: "0",
    max: "1",
    step: "0.005",
    value: String(gainToPos(gain)),
    title: silent ? "This source has no audio" : "Level",
    "data-nodrag": "",
    disabled: silent,
  });
  const gv = el("span.num.sm", { text: gainLabel(gain) });
  strip.append(fader, gv, mute);

  const parts = { fader, gv, mute, lane: null, pos: null, repeat: null };
  if (source.seekable) {
    parts.pos = el("span.num.sm.dim", { style: { minWidth: "3.4em" } });
    parts.lane = el("input", { type: "range", min: "0", max: "1000", step: "1", value: "0", title: "Position", "data-nodrag": "" });
    parts.repeat = repeatButton(source, (s, on) => deps.onRepeat && deps.onRepeat(s, on));
    const style = { bottom: "0", opacity: "1", background: "transparent" };
    node.appendChild(el("div.strip", { style }, [parts.lane, parts.pos, parts.repeat]));
  }

  mute.onclick = (e) => {
    e.stopPropagation();
    deps.onMute(source.id, !mute.classList.contains("on"));
  };
  deps.audio.bindFader(fader, source.id, "gain");
  if (parts.lane) deps.scrub.bind(parts.lane, source.id);
  return parts;
}

/** Write the fader, M and the lane from the source and the panel's view. */
export function syncStrip(tile, source, view) {
  const gain = view.gain === undefined ? (source.gain === undefined ? 1 : source.gain) : view.gain;
  if (document.activeElement !== tile.fader && !view.faderBusy) {
    const wanted = String(gainToPos(gain));
    if (tile.fader.value !== wanted) tile.fader.value = wanted;
  }
  tile.gv.textContent = gainLabel(gain);
  tile.gv.classList.toggle("muted", !!source.muted);
  tile.mute.classList.toggle("on", !!source.muted);
  tile.mute.setAttribute("aria-pressed", String(!!source.muted));
  // The reason stays on a button that has none of it to mute.
  tile.mute.title = source.has_audio === false
    ? "This source has no audio"
    : source.muted
      ? "Unmute"
      : "Mute";
  tile.strip.hidden = !view.showStrip;
  if (tile.repeat) syncRepeat(tile.repeat, source);
  if (!tile.lane) return;
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
