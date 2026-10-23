// A channel's part of the map: the address it reads, the channel, what its
// picture goes through where it is converted, and each output. Shapes are
// show.list and show.stats (docs/reference/shows.md). The inputs are added
// once every channel is in, since an address feeds many of them; so is the
// GPU, which every conversion on it uses.

import { groupKey, numbered, stateOf } from "../headend/model.js";
import { codec } from "../wall/model.js";
import { toneOf } from "./tone.js";
import { hlsFormat, inputKind, outputFamily, outputKind, picture, rate, where } from "./words.js";

/** Errors in the last few seconds, as how hard a link stutters: 0 to 1. */
function stutter(series) {
  const recent = series ? series.cc.slice(-3).reduce((n, v) => n + v, 0) : 0;
  return recent ? Math.min(1, 0.35 + recent / 20) : 0;
}

export function addDirect(g, show, st, series) {
  const state = stateOf(show, st);
  const i = (st && st.input) || {};
  const n = numbered(show);
  const key = groupKey(show);
  const id = `show:${show.id}`;
  const shape = [picture(i.width, i.height, i.fps), i.video_codec && codec(i.video_codec), i.audio_codec && codec(i.audio_codec)].filter(Boolean).join(" ");
  g.node({ id, col: 1, kind: "show", show: show.id, direct: true, tone: toneOf(state), state, num: n.num, title: n.title, group: key,
    lines: [shape || (state === "offline" ? "Stopped" : "No picture yet"), rate(i.kbps)], rate: i.kbps || 0 });
  if (key) {
    const err = stutter(series);
    g.link(`in>${show.id}`, `in:${key}`, id, i.kbps, err ? "warn" : toneOf(state === "offline" ? "off" : state === "starting" ? "starting" : "live"), { err });
  }
  const counted = new Map(((st && st.outputs) || []).map((o) => [o.id, o]));
  for (const o of show.outputs || []) {
    if (o.enabled === false) continue;
    addOutput(g, show, o, counted.get(o.id) || {}, id, i.kbps);
  }
}

/** What a conversion makes, in a broadcaster's words: "720p H.264 1.5 Mbit/s". */
export function made(o) {
  const v = (o.plan && o.plan.video) || (o.rendition && o.rendition.request && o.rendition.request.video) || (o.rendition && o.rendition.video) || null;
  if (!v) return o.rendition && o.rendition.preset ? o.rendition.preset : "Converted";
  return [picture(v.width, v.height, v.fps), v.codec && codec(v.codec), rate(v.bitrate_kbps)].filter(Boolean).join(" ");
}

function addOutput(g, show, o, s, from, inK) {
  const state = s.state || o.state || "waiting";
  const tone = toneOf(state);
  const oid = `out:${show.id}:${o.id}`;
  const steps = s.steps || [];
  const converts = !!o.rendition && s.rendition_text !== "copy";
  let up = from;
  if (converts) {
    up = `proc:${show.id}:${s.rendition_text || o.id}`;
    const gpu = steps.length ? steps.some((x) => x.gpu) : /nv|cuda|vaapi|qsv|videotoolbox|amf/i.test(s.encoder || (o.plan && o.plan.encoder) || "");
    const had = g.nodes.get(up);
    g.node({ id: up, col: 2, kind: "proc", show: show.id, tone: o.refused ? "bad" : toneOf(state === "live" ? "live" : state), title: made(o), gpu, steps,
      lines: [gpu ? "GPU" : "CPU", steps.length ? steps.map((x) => x.kind).join(", ") : "decode, scale, encode"], serves: (had ? had.serves : []).concat(o.id), encoder: s.encoder || (o.plan && o.plan.encoder), cpu: s.cpu_millicores, reason: o.plan && o.plan.encoder_reason, refused: o.refused });
    g.link(`${from}>${up}`, from, up, inK, toneOf(state === "failed" ? "live" : state));
  }
  const p = o.playback || null;
  const kbps = s.kbps || 0;
  const lines = [outputFamily(o) === "hls" ? hlsFormat(p) || "this station" : where(o), [rate(kbps), p && p.viewers ? `${p.viewers} watching` : ""].filter(Boolean).join(" · ")];
  g.node({ id: oid, col: 3, kind: "output", show: show.id, output: o, stats: s, tone, state, title: o.label && o.label !== o.id && outputFamily(o) !== "hls" ? `${outputKind(o)}: ${o.label}` : outputKind(o), family: outputFamily(o), lines, rate: kbps });
  g.link(`${up}>${oid}`, up, oid, kbps, tone, { straight: !converts });
}

/** One node per address the channels read, with what it feeds. */
export function addInputs(g, shows, stats, series) {
  const by = new Map();
  for (const s of shows) {
    if (s.compositing !== false) continue;
    const key = groupKey(s);
    if (!key) continue;
    if (!by.has(key)) by.set(key, { key, uri: s.input.uri, members: [], live: 0, kbps: 0, cc: 0, on: 0 });
    const e = by.get(key);
    const st = stats.get(s.id);
    const state = stateOf(s, st);
    e.members.push(s.id);
    if (state !== "offline") e.on += 1;
    if (state !== "offline" && state !== "starting") e.live += 1;
    e.kbps += (st && st.input && st.input.kbps) || 0;
    const h = series && series.get(s.id);
    if (h) e.cc += h.ccMinute();
  }
  for (const e of by.values()) {
    const dead = e.on > 0 && e.live === 0;
    const tone = dead ? "bad" : !e.on ? "off" : e.cc ? "warn" : e.live < e.on ? "busy" : "ok";
    const many = e.members.length;
    g.node({ id: `in:${e.key}`, col: 0, kind: "input", tone, title: e.key, uri: e.uri, members: e.members, cc: e.cc, live: e.live, rate: e.kbps, dead,
      lines: [`${inputKind(e.uri)}${many > 1 ? ` · ${many} programmes` : ""}`, [rate(e.kbps) || (dead ? "Nothing arriving" : ""), e.cc ? `${e.cc} errors a minute` : ""].filter(Boolean).join(" · ")] });
  }
}

/** The GPU as a node, linked to every conversion that runs on it. */
export function addGpu(g, status) {
  const gpus = (status && status.gpus) || [];
  const procs = [...g.nodes.values()].filter((n) => n.kind === "proc" && n.gpu);
  gpus.forEach((d, i) => {
    const s = d.now || {};
    const busy = Math.max(s.encoder_pct || 0, s.decoder_pct || 0);
    const id = `gpu:${d.index ?? i}`;
    g.node({ id, col: 2, kind: "gpu", tone: busy >= 95 ? "warn" : "ok", title: d.name || "GPU", gpuInfo: d, rate: 0,
      lines: [`Encoder ${s.encoder_pct ?? "n/a"}% · decoder ${s.decoder_pct ?? "n/a"}%`, `${s.encoder_sessions ?? 0} sessions · ${s.memory_total_mib ? `${(s.memory_used_mib / 1024).toFixed(1)} of ${(s.memory_total_mib / 1024).toFixed(0)} GB` : ""}`] });
    if (i === 0) for (const p of procs) g.link(`${id}~${p.id}`, id, p.id, 0, "ok", { uses: true });
  });
}
