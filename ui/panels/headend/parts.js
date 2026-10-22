// The detail drawer's sections: every track in and out, the transcoder
// settings of each output that converts, and the input's history since
// this page opened. Plain nodes from numbers, redrawn each second.

import { el } from "../../shell/dom.js";
import { codec, kbps } from "../wall/model.js";
import { bars } from "./charts.js";
import { soundTracks } from "./tile.js";

const table = (head, rows) => el("table.he-table", {}, [
  el("thead", {}, [el("tr", {}, head.map((h) => el("th", { text: h })))]),
  el("tbody", {}, rows.map((r) => el("tr", {}, r.map((c) => el("td", {}, [c ?? ""]))))),
]);

const db = (v) => (v == null ? "not measured" : v <= -120 ? "silence" : `${v} dBFS`);

/** The input's tracks: the station's list where it sends one, else what show.stats says. */
export function inputTracks(st) {
  const i = (st && st.input) || {};
  const listed = i.tracks || [];
  if (listed.length) {
    const peaks = soundTracks(st);
    let a = 0;
    return table(["#", "Kind", "Codec", "Language", "Shape", "Rate", "PID", "Level"], listed.map((t) => [
      String(t.track ?? ""), t.kind, codec(t.codec || ""), t.lang || "", t.kind === "video" ? [t.width && `${t.width}×${t.height}`, t.fps && `${t.fps} fps`].filter(Boolean).join(" ") : [t.channels && `${t.channels} ch`, t.sample_rate && `${t.sample_rate / 1000} kHz`].filter(Boolean).join(" "),
      t.kbps ? kbps(t.kbps) : "", t.pid != null ? String(t.pid) : "", t.kind === "audio" ? db((peaks[a++] || {}).peak_db) : "",
    ]));
  }
  if (!i.video_codec && !i.audio_codec) return el("p.he-calm", { text: "Nothing has arrived on the input yet." });
  const rows = [];
  if (i.video_codec) rows.push(["0", "video", codec(i.video_codec), "", [i.width && `${i.width}×${i.height}`, i.fps && `${Math.round(i.fps * 100) / 100} fps`].filter(Boolean).join(" "), "", ""]);
  if (i.audio_codec) rows.push(["0", "audio", codec(i.audio_codec), "", i.audio_channels ? `${i.audio_channels} ch` : "", "", db(i.audio_peak_db)]);
  return el("div", {}, [
    table(["#", "Kind", "Codec", "Shape", "Level"], rows.map((r) => [r[0], r[1], r[2], r[4], r[6]])),
    el("p.he-calm", { text: `${kbps(i.kbps || 0)} in, both together. This station carries the first video and the first sound track of a program; a station that carries every track lists each one here.` }),
  ]);
}

/** Each output: where it goes, its state, rate and format. */
export function outputTracks(show, st) {
  const by = new Map(((st && st.outputs) || []).map((o) => [o.id, o]));
  const outs = show.outputs || [];
  if (!outs.length) return el("p.he-calm", { text: "No outputs." });
  return table(["Output", "Where", "State", "Rate", "Format", "Tracks"], outs.map((o) => {
    const s = by.get(o.id) || {};
    const tracks = (s.tracks || o.tracks || []).map((t) => `${t.kind || ""} ${codec(t.codec || "")}${t.lang ? ` ${t.lang}` : ""}`.trim()).join(", ");
    return [o.id, o.uri_host || o.uri || o.platform || "", el(`span.he-ostate.${s.state || o.state || "idle"}`, { text: s.state || o.state || "" }), s.kbps ? kbps(s.kbps) : "", s.rendition_text || "copy", tracks];
  }));
}

/** The settings of every output that converts, as they were asked for. */
export function transcoder(show, st) {
  const by = new Map(((st && st.outputs) || []).map((o) => [o.id, o]));
  const conv = (show.outputs || []).filter((o) => o.rendition || (by.get(o.id) || {}).encoder);
  if (!conv.length) return el("p.he-calm", { text: "Every output copies the input as it came: nothing is decoded or encoded for this channel." });
  return el("div.he-trans", {}, conv.map((o) => {
    const s = by.get(o.id) || {};
    const pairs = flat(o.rendition || {});
    if (s.encoder) pairs.unshift(["encoder", s.encoder]);
    if (s.cpu_millicores) pairs.push(["cpu", `${(s.cpu_millicores / 10).toFixed(0)}% of a core`]);
    return el("div", {}, [el("h5", { text: `${o.id}: ${s.rendition_text || "converted"}` }), el("dl.he-kv", {}, pairs.flatMap(([k, v]) => [el("dt", { text: k }), el("dd", { text: v })]))]);
  }));
}

function flat(obj, pre = "") {
  return Object.entries(obj).flatMap(([k, v]) => (v && typeof v === "object" && !Array.isArray(v) ? flat(v, `${pre}${k} `) : [[`${pre}${k}`, Array.isArray(v) ? v.map((x) => (typeof x === "object" ? JSON.stringify(x) : x)).join(", ") : String(v)]]));
}

/** Two minutes of errors per second, and what happened since the page opened. */
export function history(series) {
  const b = bars(240, 28);
  b.set(series ? series.cc : []);
  const total = series ? series.cc.reduce((n, v) => n + v, 0) : 0;
  const log = series && series.log.length
    ? el("ol.he-log", {}, series.log.slice(0, 30).map((e) => el(`li.${e.kind}`, {}, [el("time", { text: new Date(e.at).toLocaleTimeString() }), " ", e.text])))
    : el("p.he-calm", { text: "Nothing has happened to this input since this page opened." });
  return el("div", {}, [
    el("div.he-ccline", {}, [b.node, el("span", { text: total ? `${total} in two minutes` : "no continuity errors in two minutes" })]),
    log,
  ]);
}
