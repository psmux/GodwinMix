// What headend.import reports, drawn: one line of totals, a row per
// stream with the show it becomes (its input, what its HLS output serves
// and the rendition), the refusals, and every line of the config that is
// not carried. Plain nodes from the answer; nothing here calls the station.

import { el } from "../../shell/dom.js";
import { describe } from "../renditions/model.js";

const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

/** "H.264 1280×720 · 1.5 Mb/s · keyframe every 50 frames · pad #000000 · AAC 128 kb/s, every track". */
export function renditionWords(r) {
  if (!r) return "Copy, as it comes";
  if (r.preset) return r.preset;
  const v = r.video || {};
  const a = r.audio || {};
  const d = describe(r);
  const bits = [v.codec ? v.codec.toUpperCase().replace("H264", "H.264") : "", d.size, d.fps, v.bitrate_kbps ? d.bitrate : ""];
  if (v.keyframe_frames) bits.push(`keyframe every ${v.keyframe_frames} frames`);
  if (v.fit) bits.push(v.fit === "pad" ? `pad ${v.pad_color || "#000000"}` : v.fit);
  if (v.deinterlace && v.deinterlace !== "off") bits.push(`deinterlace ${v.deinterlace}`);
  if (v.preset) bits.push(`preset ${v.preset}`);
  const sound = a.codec ? `${a.codec.toUpperCase()}${a.bitrate_kbps ? ` ${a.bitrate_kbps} kb/s` : ""}${a.tracks === "all" ? ", every track" : ""}` : r.no_audio ? "no sound" : "";
  return [...bits, sound].filter(Boolean).join(" · ") || "Re-encoded";
}

/** "MPEG-TS and CMAF, open, 3.4 s segments". */
export function hlsWords(p) {
  const q = p || {};
  const seg = { both: "MPEG-TS and CMAF", ts: "MPEG-TS", cmaf: "CMAF" }[q.segments || "cmaf"];
  const who = q.access === "open" ? "open on the network" : "keyed";
  return [seg, who, q.segment_ms ? `${(q.segment_ms / 1000).toFixed(1)} s segments` : "", q.window ? `${q.window} s window` : ""].filter(Boolean).join(", ");
}

export function inputWords(i) {
  if (!i) return "";
  const nic = i.params && i.params.interface ? ` on ${i.params.interface}` : "";
  return `${i.uri || ""}${i.program ? `, program ${i.program}` : ""}${nic}${i.backup ? `, backup ${i.backup.uri}` : ""}`;
}

/** The totals, one sentence. */
export function summary(r) {
  const refused = (r.shows && r.shows.refused) || [];
  const parts = [`${plural(r.streams.length, "stream", "streams")}: ${r.converted} converted, ${r.copied} copied`];
  if (r.disabled) parts.push(`${r.disabled} off`);
  const lead = parts.join(", ") + ".";
  if (!r.dry_run) return `${lead} ${plural((r.shows.added || []).length, "show was", "shows were")} made${refused.length ? `, ${refused.length} refused` : ""}.`;
  return `${lead} ${refused.length ? `${refused.length} would be refused.` : "Every one would be made."}`;
}

/** The table of streams, a refused one marked with why. */
export function streamTable(r) {
  const why = new Map(((r.shows && r.shows.refused) || []).map((x) => [x.name, x.why]));
  const rows = r.streams.map((s) => {
    const hls = (s.outputs || [])[0] || {};
    const pushes = (s.outputs || []).length - 1;
    const no = why.get(s.name);
    return el(`tr${no ? ".he-imp-refused" : ""}`, {}, [
      el("td", {}, [el("strong", { text: s.name }), s.title ? el("div.he-sub", { text: s.title }) : null, s.disabled ? el("div.he-sub", { text: "off" }) : null]),
      el("td.he-imp-wrap", { text: inputWords(s.input) }),
      el("td.he-imp-wrap", { text: `${hlsWords(hls.params)}${pushes > 0 ? `, and ${plural(pushes, "push", "pushes")}` : ""}` }),
      el("td.he-imp-wrap", {}, [renditionWords(hls.rendition), no ? el("div.he-imp-why", { text: no }) : null]),
    ]);
  });
  return el("div.he-imp-scroll", {}, [el("table.he-table.he-imp-table", {}, [
    el("thead", {}, [el("tr", {}, ["Stream", "Input", "HLS output", "Rendition"].map((h) => el("th", { text: h })))]),
    el("tbody", {}, rows),
  ])]);
}

/** Every line the import does not carry, in a closed disclosure. */
export function notesList(r) {
  const notes = r.notes || [];
  if (!notes.length) return el("p.he-calm", { text: "Every line of the config is carried." });
  const kind = { ignored: "not used", not_carried: "done differently" };
  return el("details.he-imp-notes", {}, [
    el("summary", { text: `Lines not carried (${notes.length})` }),
    el("ul.sm", {}, notes.map((n) => el("li", { text: `${n.line ? `line ${n.line}, ` : ""}${n.scope}, ${kind[n.kind] || n.kind}: ${n.what}` }))),
  ]);
}
