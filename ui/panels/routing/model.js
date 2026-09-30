// What the routing view draws, worked out from what the core answered.
// Pure: no DOM, no calls, so the tests read it directly.
//
// Inputs run down the side and outputs across, both in groups: one group
// per channel (its streams, its destinations) and one per show (its
// programme and sources, its outputs). A cell is a route when the output
// reads that input, an offer when it could with one change, a note when a
// source reaches the output through the programme, and nothing otherwise.

import { nodesFor, encoderOf } from "../renditions/words.js";

/** Groups in the order they are drawn: channels, then shows. */
export function groups(channels, shows) {
  const out = [];
  for (const c of channels || []) out.push(channelGroup(c));
  for (const s of shows || []) out.push(showGroup(s));
  return out;
}

const shape = (v) => (v && v.width ? `${v.width}×${v.height}${v.fps ? ` ${Math.round(typeof v.fps === "number" ? v.fps : v.fps.num / v.fps.den)} fps` : ""}` : "");

function channelGroup(c) {
  const streams = (c.streams || []).filter((s) => s.state !== "gone");
  const rows = streams.map((s) => ({ key: `ch:${c.id}/${s.name}`, id: s.name, label: s.name, sub: [String(s.protocol || "").toUpperCase(), shape(s.video)].filter(Boolean).join(" · "), live: s.state === "live", stream: s }));
  if (!rows.length) rows.push({ key: `ch:${c.id}/*`, id: "*", label: "Nothing publishing", sub: "Waiting for a stream", idle: true });
  const cols = (c.destinations || []).map((d) => ({ key: `chd:${c.id}/${d.id}`, id: d.id, label: d.label || d.platform || d.id, sub: d.platform || "", state: d.state, dest: d }));
  return { kind: "channel", id: c.id, key: `c:${c.id}`, name: c.name || c.id, rows, cols, channel: c };
}

function showGroup(s) {
  const d = s.detail || null;
  const rows = [{ key: `show:${s.id}:programme`, id: "programme", label: "Programme", sub: s.on_air ? `On air: ${s.on_air}` : "Nothing on air", programme: true, live: !!s.on_air }];
  for (const src of (d && d.sources) || []) {
    rows.push({ key: `show:${s.id}:src:${src.id}`, id: src.id, label: src.name || src.id, sub: d.tally && d.tally[src.id] === "program" ? "In the programme" : "", source: src, onAir: !!(d.tally && d.tally[src.id] === "program") });
  }
  const cols = ((d && d.outputs) || []).map((o) => ({ key: `out:${s.id}/${o.id}`, id: o.id, label: o.id, sub: host(o), state: o.state, output: o }));
  return { kind: "show", id: s.id, key: `s:${s.id}`, name: s.name || s.id, state: s.state, rows, cols, show: s, loaded: !!d };
}

const host = (o) => (o.type === "hls/output" ? "HLS for viewers" : o.type === "record/output" ? "Recording" : String(o.uri_host || "").replace(/^\w+:\/\//, "").replace(/[/:].*$/, ""));

/** The stream a channel destination reads now: its own, or the one `*` resolves to. */
export function streamOf(group, dest) {
  if (dest.stream && dest.stream !== "*") return dest.stream;
  if (dest.plan && dest.plan.stream) return dest.plan.stream;
  const live = group.rows.filter((r) => r.live).sort((a, b) => (a.stream.since_ms || 0) - (b.stream.since_ms || 0));
  return live.length ? live[0].id : "*";
}

/**
 * What one cell holds.
 * @returns {{kind: "route"|"offer"|"via"|"none", format?, where?, cost?, why?}}
 */
export function cell(rowGroup, row, colGroup, col, plans) {
  if (rowGroup.key !== colGroup.key) return { kind: "none" };
  const plan = plans && plans[colGroup.key];
  if (colGroup.kind === "channel") {
    const reads = streamOf(colGroup, col.dest);
    if (reads === row.id) return route(plan, col.id, col.dest.rendition, col.dest.plan, true);
    return row.idle ? { kind: "none" } : { kind: "offer" };
  }
  if (row.programme) return route(plan, col.id, col.output.rendition, null, false);
  return row.onAir ? { kind: "via" } : { kind: "none" };
}

function route(plan, id, rendition, destPlan, copyable) {
  const { nodes, video, audio } = nodesFor(plan, id);
  const enc = encoderOf(video || audio);
  const cpu = nodes.reduce((sum, n) => sum + ((n.cost && n.cost.cpu_millicores) || 0) / Math.max(1, (n.serves || []).length), 0);
  const shared = Math.max(0, ...nodes.filter((n) => encoderOf(n)).map((n) => (n.serves || []).length - 1));
  if (!rendition || (destPlan && destPlan.mode === "copy") || (nodes.length && !enc)) {
    return copyable
      ? { kind: "route", format: "Copy", where: "no encode", cost: 0, tone: "copy" }
      : { kind: "route", format: "Programme encode", where: "no extra encode", cost: 0, tone: "prog" };
  }
  const out = (destPlan && destPlan.video) || null;
  const format = formatWords(rendition, out);
  if (!enc) return { kind: "route", format, where: "planned when live", cost: 0, tone: "wait" };
  const where = `${enc.hardware ? "GPU" : "CPU"}, ${encoderName(enc.id)}${shared ? `, shared by ${shared + 1}` : ""}`;
  return { kind: "route", format, where, cost: cpu, tone: enc.hardware ? "gpu" : "cpu" };
}

/** A catalogue id short enough for a cell: `h264-videotoolbox` is VideoToolbox. */
export function encoderName(id) {
  const bare = String(id || "").replace(/^(h264|h265|hevc|av1|vp8|vp9)-/, "").replace(/^software-/, "");
  const known = { videotoolbox: "VideoToolbox", nvenc: "NVENC", nvidia: "NVENC", vaapi: "VA-API", va: "VA-API", qsv: "Quick Sync", amf: "AMF", v4l2: "V4L2" };
  return known[bare] || bare;
}

/** "YouTube 720p", or "1280×720 30 fps · 3 Mb/s" for a request written out. */
export function formatWords(rendition, out) {
  if (rendition && rendition.preset) return presetWords(rendition.preset);
  if (rendition && rendition.ladder) return `Ladder of ${rendition.ladder.length}`;
  const v = out || (rendition && rendition.video) || {};
  const kbps = v.bitrate_kbps ? ` · ${v.bitrate_kbps >= 1000 ? `${Math.round(v.bitrate_kbps / 100) / 10} Mb/s` : `${v.bitrate_kbps} kb/s`}` : "";
  return (shape(v) || (rendition && rendition.no_video ? "Sound only" : "Converted")) + kbps;
}

/** `youtube-720p30` as a person reads it: "YouTube 720p30". */
export function presetWords(id) {
  const names = { youtube: "YouTube", facebook: "Facebook", twitch: "Twitch", abr: "Ladder", audio: "Sound only", copy: "Copy" };
  const [first, ...rest] = String(id).split("-");
  if (first === "audio") return "Sound only";
  if (first === "copy") return "Copy";
  return [names[first] || first, ...rest.filter((r) => !/^(ladder|only|aac)$/.test(r))].join(" ");
}

/** Millicores, short enough for a cell: "free", "12% CPU", "1.4 cores". */
export function costWords(m) {
  if (!m) return "free";
  if (m < 1000) return `${Math.max(1, Math.round(m / 10))}% of a core`;
  return `${Math.round(m / 100) / 10} cores`;
}

/** Only the rows and columns whose names, or whose group's name, match. */
export function filtered(list, text) {
  const q = String(text || "").trim().toLowerCase();
  if (!q) return list;
  const hit = (s) => String(s || "").toLowerCase().includes(q);
  return list.map((g) => {
    if (hit(g.name)) return g;
    const rows = g.rows.filter((r) => hit(r.label) || hit(r.sub));
    const cols = g.cols.filter((c) => hit(c.label) || hit(c.sub));
    // An output that matches is shown with every input it could read, and
    // the other way round, or the match would be a column of empty cells.
    return { ...g, rows: rows.length || !cols.length ? rows : g.rows, cols: cols.length || !rows.length ? cols : g.cols };
  }).filter((g) => g.rows.length || g.cols.length);
}
