// One channel's tile: its picture with the state ring round it, its number
// and name, a level meter per sound track, codec and size badges, and its
// input and output rates as sparklines with the continuity errors of the
// last minute. Built once per channel on screen and then patched each
// second, so the only motion is what changed: a tile pulses when its state
// moves, and the meters move with the sound.

import { el } from "../../shell/dom.js";
import { codec, kbps } from "../wall/model.js";
import { spark } from "./charts.js";
import { WORD, numbered } from "./model.js";

/** dBFS to the height of a meter: -60 is empty, 0 is full. */
export const meterOf = (db) => (db == null ? null : Math.max(0, Math.min(1, (db + 60) / 60)));

/** The sound tracks to meter: the input's own list where the station sends one. */
export function soundTracks(st) {
  const i = (st && st.input) || {};
  const listed = (i.tracks || []).filter((t) => t.kind === "audio");
  if (listed.length) return listed.map((t, n) => ({ ...t, peak_db: t.peak_db ?? (n === 0 ? i.audio_peak_db : null) }));
  return i.audio_codec ? [{ codec: i.audio_codec, channels: i.audio_channels, peak_db: i.audio_peak_db }] : [];
}

export class Tile {
  constructor(id, thumbs) {
    this.id = id;
    this.state = null;
    this.badge = el("span.he-state");
    this.num = el("span.he-num");
    this.word = el("span.he-picword");
    this.meters = el("div.he-meters", { "aria-hidden": "true" });
    this.badges = el("div.he-badges");
    this.title = el("span.he-title");
    this.sub = el("span.he-sub");
    this.inS = spark("in");
    this.outS = spark("out");
    this.inV = el("span.he-v");
    this.outV = el("span.he-v");
    this.cc = el("span.he-cc");
    this.pic = el("div.he-pic", {}, [thumbs.img(id), this.word, this.badge, this.num, this.meters, this.badges]);
    this.node = el("div.he-tile", { "data-id": id, role: "button", tabindex: "-1" }, [
      this.pic,
      el("div.he-info", {}, [
        el("div.he-head", {}, [this.title, this.cc]),
        this.sub,
        el("div.he-rates", {}, [
          el("span.he-rate", { title: "Input" }, [el("span.he-k", { text: "in" }), this.inS.node, this.inV]),
          el("span.he-rate", { title: "Outputs, summed" }, [el("span.he-k", { text: "out" }), this.outS.node, this.outV]),
        ]),
      ]),
    ]);
    this.node.addEventListener("animationend", () => this.node.classList.remove("he-pulse"));
  }

  /** `row` is {show, st, state}; `series` the channel's history. */
  update(row, series, selected) {
    const { show, st, state } = row;
    if (state !== this.state) {
      if (this.state) this.node.classList.add("he-pulse");
      this.node.classList.remove(`s-${this.state}`);
      this.node.classList.add(`s-${state}`);
      this.state = state;
      this.badge.textContent = WORD[state];
      this.word.textContent = state === "live" || state === "silent" ? "" : WORD[state];
    }
    this.node.classList.toggle("selected", !!selected);
    const n = numbered(show);
    this.num.textContent = n.num;
    if (this.title.textContent !== n.title) this.title.textContent = n.title;
    const i = (st && st.input) || {};
    this.sub.textContent = [show.input && show.input.program ? `program ${show.input.program}` : "", i.fps ? `${Math.round(i.fps * 100) / 100} fps` : ""].filter(Boolean).join(" · ") || (show.input ? show.input.uri : "");
    this.inS.set(series && series.inK.slice(-60));
    this.outS.set(series && series.outK.slice(-60));
    this.inV.textContent = st ? kbps(i.kbps || 0) : "";
    this.outV.textContent = st ? kbps((st.outputs || []).reduce((t, o) => t + (o.kbps || 0), 0)) : "";
    const cc = series ? series.ccMinute() : 0;
    this.cc.textContent = cc ? `${cc} CC` : "";
    this.cc.title = cc ? `${cc} continuity errors or lost packets in the last minute` : "";
    this.drawBadges(i, st);
    this.drawMeters(soundTracks(st));
  }

  drawBadges(i, st) {
    const out = ((st && st.outputs) || []).map((o) => o.rendition_text).find((t) => t && t !== "copy");
    const bits = [i.video_codec && codec(i.video_codec), i.height ? `${i.height}p` : "", i.audio_codec && codec(i.audio_codec), out ? `to ${out}` : ""].filter(Boolean);
    const key = bits.join("|");
    if (this.badges.dataset.key === key) return;
    this.badges.dataset.key = key;
    this.badges.replaceChildren(...bits.map((b) => el("span.he-badge", { text: b })));
  }

  drawMeters(tracks) {
    if (this.meters.childElementCount !== tracks.length) {
      this.meters.replaceChildren(...tracks.map(() => el("span.he-meter", {}, [el("span.he-fill")])));
    }
    tracks.forEach((t, n) => {
      const m = this.meters.children[n];
      const f = meterOf(t.peak_db);
      m.classList.toggle("unread", f == null);
      m.classList.toggle("quiet", t.peak_db != null && t.peak_db <= -60);
      m.firstChild.style.transform = `scaleY(${f ?? 0})`;
      m.title = `${t.lang || `Track ${n + 1}`} ${codec(t.codec || "")}${t.channels ? ` ${t.channels}ch` : ""}: ${t.peak_db == null ? "not measured" : `${t.peak_db} dBFS`}`;
    });
  }
}
