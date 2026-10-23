// The body of each hover card, by what is under the pointer. Each returns
// rows of [label, value] for the top, in a broadcaster's words, and rows
// for a "Technical" part below for whoever wants the detail.

import { codec, kbps as shortRate } from "../wall/model.js";
import { WORD as STATE } from "../headend/model.js";
import { picture, rate, segmentWords, sourceKind, stateWord, upFor, where } from "./words.js";

const yes = (rows) => rows.filter((r) => r && r[1] !== "" && r[1] != null);

export function inputRows(n, ctx) {
  const names = (n.members || []).map((id) => (ctx.find(id) || {}).name || id);
  const list = names.slice(0, 8).join(", ") + (names.length > 8 ? `, and ${names.length - 8} more` : "");
  return {
    top: yes([["Address", n.title], ["Kind", n.lines[0]], ["Rate", rate(n.rate) || "nothing arriving"], ["Errors", n.cc ? `${n.cc} in the last minute` : "none in the last minute"], ["Feeds", list], ["Live", `${n.live} of ${(n.members || []).length}`]]),
    tech: yes([["Input", n.uri]]),
  };
}

export function sourceRows(n) {
  const s = n.source || {};
  return {
    top: yes([["Kind", sourceKind(s)], ["State", stateWord(s.state)], ["Tally", n.tally === "program" ? "On air" : n.tally === "preview" ? "In preview" : "Not on air"],
      ["Carries", [s.has_video && "picture", s.has_audio && "sound"].filter(Boolean).join(" and ") || "nothing yet"], s.muted ? ["Sound", "muted"] : null]),
    tech: yes([["Id", s.id], ["Address", s.uri], ["Last picture", s.video_idle_ms != null ? `${s.video_idle_ms} ms ago` : ""], s.backup ? ["Backup", s.backup.state || ""] : null]),
  };
}

function tracksIn(i) {
  const listed = i.tracks || [];
  if (listed.length) return listed.map((t) => `${t.kind === "video" ? picture(t.width, t.height, t.fps) : `${t.channels || ""} ch`} ${codec(t.codec || "")}${t.lang ? ` ${t.lang}` : ""}`.trim()).join(", ");
  return [i.video_codec && `${picture(i.width, i.height, i.fps)} ${codec(i.video_codec)}`, i.audio_codec && `${codec(i.audio_codec)} ${i.audio_channels || ""} ch`].filter(Boolean).join(", ");
}

export function showRows(n, ctx) {
  const show = ctx.find(n.show) || {};
  const st = ctx.stats.get(n.show) || {};
  if (n.mixed) {
    const d = ctx.mixed.get(n.show) || {};
    return {
      top: yes([["State", stateWord(show.state === "running" ? "live" : show.state)], ["On air", n.onAir || "black"], ["Preview", n.preview || ""], ["Sources", String((d.sources || []).length)],
        ["Outputs", String((d.outputs || []).length)], ["Sending", rate(show.programme_kbps)], ["Up", upFor(d.uptime_secs)]]),
      tech: yes([["Show", show.id], ["CPU", show.cpu_millicores != null ? `${(show.cpu_millicores / 10).toFixed(0)}% of a core` : ""], ["Memory", show.memory_mib ? `${show.memory_mib} MiB` : ""], ["Restarts", show.restarts ? String(show.restarts) : ""], ["Encoder", d.backend && d.backend.video_encoder]]),
    };
  }
  const i = st.input || {};
  const outs = st.outputs || [];
  return {
    top: yes([["State", STATE[n.state] || stateWord(n.state)], ["Input", show.input ? `${show.input.uri}${show.input.program ? `, programme ${show.input.program}` : ""}` : "none"], ["Tracks in", tracksIn(i)],
      ["Rate in", rate(i.kbps)], ["Rate out", rate(outs.reduce((s, o) => s + (o.kbps || 0), 0))], ["Outputs", outs.map((o) => o.id).join(", ")], ["Alarms", ((show.health && show.health.alarms) || []).map((a) => a.detail || a.kind).join("; ")]]),
    tech: yes([["Show", show.id], ["Errors", i.cc_errors || i.packets_lost ? `${i.cc_errors || 0} continuity, ${i.packets_lost || 0} lost packets in all` : ""], ["Keyframe", i.keyframe_ms ? `every ${(i.keyframe_ms / 1000).toFixed(1)} s` : ""], ["Work", st.work || ""], ["CPU", st.cpu_millicores ? `${(st.cpu_millicores / 10).toFixed(0)}% of a core` : ""]]),
  };
}

const STEP = { decode: "Decode", scale: "Scale", encode: "Encode" };

export function procRows(n, ctx) {
  const gpu = ctx.gpu && ctx.gpu.gpus && ctx.gpu.gpus[0];
  const g = (gpu && gpu.now) || {};
  return {
    top: yes([["Makes", n.makes || n.title], ["Runs on", n.gpu ? (gpu ? gpu.name : "the GPU") : "the CPU"], ["For", (n.serves || []).join(", ")],
      n.gpu && gpu ? ["GPU load", `encoder ${g.encoder_pct ?? "n/a"}%, decoder ${g.decoder_pct ?? "n/a"}%, ${g.encoder_sessions ?? 0} sessions`] : null, n.refused ? ["Refused", n.refused.message] : null, n.shed ? ["Paused", n.shed] : null]),
    tech: yes([...(n.steps || []).map((s) => [STEP[s.kind] || s.kind, `${s.element || "chosen by the host"}${s.gpu ? ", on the GPU" : ""}`]), ["Encoder", n.encoder], ["Why", n.reason], ["CPU", n.cpu ? `${(n.cpu / 10).toFixed(0)}% of a core` : ""]]),
  };
}

export function programRows(n) {
  const b = n.backend || {};
  return {
    top: yes([["On air", n.onAir || "black"], ["Preview", n.preview || ""], ["Sending", rate(n.rate)], ["Encode", b.video_encoder ? (b.hardware_accelerated ? "on the GPU" : "on the CPU") : ""]]),
    tech: yes([["Video", [b.video_decoder, b.video_encoder].filter(Boolean).join(" to ")], ["Sound", [b.audio_decoder, b.audio_encoder].filter(Boolean).join(" to ")]]),
  };
}

export function gpuRows(n, ctx) {
  const d = n.gpuInfo || {};
  const s = d.now || {};
  const uses = (ctx.graph.from.get(n.id) || []).length;
  return {
    top: yes([["Encoder", s.encoder_pct != null ? `${s.encoder_pct}%` : ""], ["Decoder", s.decoder_pct != null ? `${s.decoder_pct}%` : ""], ["3D", s.gpu_pct != null ? `${s.gpu_pct}%` : ""],
      ["Memory", s.memory_total_mib ? `${s.memory_used_mib} of ${s.memory_total_mib} MiB` : ""], ["Sessions", s.encoder_sessions != null ? String(s.encoder_sessions) : ""], ["Converting", `${uses} ${uses === 1 ? "output" : "outputs"}`]]),
    tech: yes([["Driver", d.driver], ["Temperature", s.temperature_c != null ? `${s.temperature_c} °C` : ""], ["Power", s.power_w != null ? `${Math.round(s.power_w)} W` : ""], ["Clock", s.graphics_mhz ? `${s.graphics_mhz} MHz` : ""]]),
  };
}

export function outputRows(n) {
  const o = n.output || {}, s = n.stats || {}, p = o.playback || null;
  return {
    top: yes([["Kind", n.title], ["Format", segmentWords(p)], ["Access", p && p.access ? (p.access === "open" ? "open, no key needed on this network" : "keyed, a player needs the viewer key") : ""], ["Segments", p && p.segment_ms ? `${(p.segment_ms / 1000).toFixed(1)} s` : ""], ["Goes to", where(o)], ["State", stateWord(n.state)],
      ["Rate", rate(n.rate) || (n.state === "live" ? "not measured" : "")], ["Watching", p ? String(p.viewers || 0) : ""], ["Problem", o.error ? String(o.error.message || o.error) : o.refused ? o.refused.message : ""]]),
    tech: yes([["Output", o.id], ["Address", o.uri_host], ["Made as", s.rendition_text], ["Encoder", s.encoder], ["Reconnects", s.reconnects || o.reconnects ? String(s.reconnects || o.reconnects) : ""]]),
  };
}

export function linkRows(l, ctx) {
  const a = ctx.graph.nodes.get(l.a) || {}, b = ctx.graph.nodes.get(l.b) || {};
  return {
    top: yes([["From", a.title], ["To", b.title], ["Rate", l.nominal ? (l.kbps ? "flowing, not measured" : "nothing") : shortRate(l.kbps) ? rate(l.kbps) : "nothing"],
      ["State", l.err ? "errors in the last seconds" : l.uses ? "uses" : !l.kbps ? "no signal" : "flowing"]]),
    tech: yes([["Link", l.straight ? "a copy, as it arrives" : ""]]),
  };
}
