// The flow map's words: what a broadcaster calls each thing on it. A rate
// is "3 Mbit/s", a picture "720p", an output "YouTube" or "Recording", a
// source "Camera" or "Browser". Internal names stay off the screen; the
// technical detail goes to the bottom of a hover card.

import { platform, platformOfHost } from "../../client/destinations.js";
import { kindOfSource } from "../../client/kind-look.js";

/** "850 kbit/s", "3.2 Mbit/s", "48 Mbit/s", "1.2 Gbit/s". */
export function rate(k) {
  if (!k) return "";
  if (k >= 1e6) return `${(k / 1e6).toFixed(1)} Gbit/s`;
  if (k >= 1e4) return `${Math.round(k / 1000)} Mbit/s`;
  if (k >= 1000) return `${(k / 1000).toFixed(1)} Mbit/s`;
  return `${Math.round(k)} kbit/s`;
}

/** "1080i", "720p50", "576p": the height, and the rate where it is not 25 or 30. */
export function picture(w, h, fps, interlaced) {
  if (!h) return "";
  const f = fps && typeof fps === "object" ? fps.num / (fps.den || 1) : fps;
  const round = f ? Math.round(f * 100) / 100 : 0;
  const plain = !round || [25, 29.97, 30].includes(round);
  return `${h}${interlaced ? "i" : "p"}${plain ? "" : Math.round(round)}`;
}

const SOURCE_WORD = { camera: "Camera", screen: "Screen", mic: "Microphone", pattern: "Test pattern", media: "Picture", page: "Browser", stream: "Stream", file: "File", exec: "Program", graphic: "Graphic", text: "Text" };
const STREAM_WORD = [[/^ndi:/i, "NDI"], [/^srt:/i, "SRT"], [/^rtmps?:/i, "RTMP"], [/^rtsp:/i, "RTSP"], [/^rist:/i, "RIST"], [/^udp:\/\/@?2(2[4-9]|3\d)\./i, "Multicast"], [/^udp:/i, "UDP"], [/\.m3u8/i, "HLS"], [/^channel:/i, "Ingest"]];

/** What a source is, in one word: "Camera", "SRT", "Browser". */
export function sourceKind(s) {
  const type = String((s && (s.type || s.kind)) || "");
  if (/^ndi/.test(type)) return "NDI";
  const kind = kindOfSource(s);
  if (kind === "stream" || kind === "file") {
    const hit = STREAM_WORD.find(([re]) => re.test(String((s && s.uri) || "")));
    if (hit) return hit[1];
  }
  if (/graphic|overlay|lower/.test(type)) return "Graphic";
  return SOURCE_WORD[kind] || "Source";
}

/** What an input address is: "Multicast", "SRT", "RTMP". */
export function inputKind(uri) {
  const hit = STREAM_WORD.find(([re]) => re.test(String(uri || "")));
  if (hit) return hit[1];
  return /^file:|^\//.test(String(uri || "")) ? "File" : "Input";
}

/** The kind an output is filtered by: hls, rtmp, srt, udp, record, other. */
export function outputFamily(o) {
  const p = String(o.platform || "");
  const host = String(o.uri_host || o.uri || "").toLowerCase();
  if (p === "hls" || host.startsWith("hls:")) return "hls";
  if (p === "file" || /^file:|record/.test(host) || o.kind === "record") return "record";
  if (p === "srt" || host.startsWith("srt:")) return "srt";
  if (/^(udp|rtp|rist):/.test(host) || p === "udp") return "udp";
  if (/^rtmps?:/.test(host) || platform(p)) return "rtmp";
  if (o.kind === "multiview") return "multiview";
  return "other";
}

export const FAMILIES = [["hls", "HLS"], ["rtmp", "RTMP"], ["srt", "SRT"], ["udp", "UDP"], ["record", "Recording"], ["multiview", "Multiview"], ["other", "Other"]];

/** "YouTube", "HLS", "SRT", "Recording": an output's name as its kind. */
export function outputKind(o) {
  const fam = outputFamily(o);
  if (fam === "hls") return "HLS";
  if (fam === "record") return "Recording";
  if (fam === "multiview") return "Multiview";
  if (fam === "srt") return "SRT";
  if (fam === "udp") return /^udp:\/\/@?2(2[4-9]|3\d)\./.test(String(o.uri_host || o.uri || "")) ? "Multicast" : "UDP";
  const known = platform(o.platform) || platformOfHost(o.uri_host || o.uri || "");
  return known && !/^custom/.test(known.id) && known.id !== "rtmp" ? known.title : "RTMP";
}

/** "CMAF and TS, open", for an HLS output whose station says. */
export function hlsFormat(p) {
  if (!p || !p.segments) return "";
  const seg = { cmaf: "CMAF", ts: "TS", both: "CMAF and TS" }[p.segments] || p.segments;
  return `${seg}, ${p.access === "open" ? "open" : "keyed"}`;
}

/** Where an output goes, without a key: the host and port, or a file's name. */
export function where(o) {
  const h = String(o.uri_host || o.uri || "");
  if (/^hls:/.test(h) || o.platform === "hls") return "this station";
  return h.replace(/^[a-z]+:\/\//i, "").replace(/\/.*$/, "");
}

const STATE_WORD = { live: "Live", starting: "Starting", nopicture: "No picture", silent: "Silent", scrambled: "Scrambled", offline: "Stopped", black: "Black", frozen: "Frozen",
  connecting: "Connecting", waiting: "Waiting", reconnecting: "Reconnecting", failed: "Failed", stopped: "Stopped", off: "Off", stalled: "Stalled", idle: "Idle", dead: "No signal", warn: "Errors" };

export const stateWord = (s) => STATE_WORD[s] || (s ? s[0].toUpperCase() + s.slice(1) : "");

/** "2 h 5 min" from seconds. */
export function upFor(secs) {
  if (secs == null) return "";
  const d = Math.floor(secs / 86400), h = Math.floor((secs % 86400) / 3600), m = Math.floor((secs % 3600) / 60);
  return d ? `${d} d ${h} h` : h ? `${h} h ${m} min` : m ? `${m} min` : `${Math.round(secs)} s`;
}
