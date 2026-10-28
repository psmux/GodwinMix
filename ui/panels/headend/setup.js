// The headend's own setup with no DOM: an input address built from the
// fields a person fills in and read back into them, a channel made from
// the editor's answers, a conversion made from the custom settings and
// read back, where a channel came from, and the names a scan gives the
// programs it found. Shapes are headend.channel.list's
// (crates/godwinmix-protocol/src/headend/channel.rs).

import { renditionWords } from "./import-report.js";

/** A channel's public name: the path every player uses. */
export const NAME = /^[A-Za-z0-9._-]{1,64}$/;

export const TRANSPORTS = [
  ["udp-m", "UDP multicast (MPEG-TS)"],
  ["rtp-m", "RTP multicast (MPEG-TS)"],
  ["udp-u", "UDP unicast to this machine"],
  ["srt", "SRT"],
  ["rtmp", "RTMP"],
  ["rtsp", "RTSP"],
  ["hls", "HLS pull"],
  ["file", "A file on the station"],
  ["other", "Another address, as typed"],
];

/** Which fields each transport asks for. */
export const ASKS = {
  "udp-m": ["group", "port", "interface", "program"],
  "rtp-m": ["group", "port", "interface", "program"],
  "udp-u": ["port", "program"],
  srt: ["address", "passphrase", "latency_ms", "program"],
  rtmp: ["url"],
  rtsp: ["url"],
  hls: ["url"],
  file: ["path", "program"],
  other: ["url", "interface", "program"],
};

const MULTICAST = /^(22[4-9]|23\d)\./;
const SCHEME = { rtmp: /^rtmps?:\/\//i, rtsp: /^rtsps?:\/\//i, hls: /^https?:\/\//i };
const PREFIX = { rtmp: "rtmp://", rtsp: "rtsp://", hls: "https://" };
const str = (v) => String(v ?? "").trim();

/** The address a channel's input takes, from the fields. "" when one is missing. */
export function inputUri(f) {
  const port = str(f.port);
  const group = str(f.group);
  switch (f.kind) {
    case "udp-m": return group && port ? `udp://@${group}:${port}` : "";
    case "rtp-m": return group && port ? `rtp://@${group}:${port}` : "";
    case "udp-u": return port ? `udp://0.0.0.0:${port}` : "";
    case "srt": {
      const a = str(f.address);
      if (!a) return "";
      return a.startsWith(":") ? `srt://@${a}` : `srt://${a.replace(/^srt:\/\//i, "")}`;
    }
    case "rtmp": case "rtsp": case "hls": {
      const u = str(f.url);
      return !u ? "" : SCHEME[f.kind].test(u) ? u : PREFIX[f.kind] + u.replace(/^[a-z]+:\/\//i, "");
    }
    case "file": {
      const p = str(f.path);
      return !p ? "" : /^file:\/\//i.test(p) ? p : `file://${p.startsWith("/") ? "" : "/"}${p}`;
    }
    default: return str(f.url);
  }
}

/** The fields an input's address and params fill, so an edit opens on them. */
export function inputFields(input) {
  const i = input || {};
  const uri = str(i.uri);
  const p = i.params || {};
  const f = { kind: "other", url: uri, interface: str(i.interface || p.interface), program: i.program ? String(i.program) : "" };
  let m = /^(udp|rtp):\/\/(?:([^@/]*)@)?\[?([^\]/:?]*)\]?:(\d+)/i.exec(uri);
  if (m) {
    const [, scheme, nic, host, port] = m;
    const many = MULTICAST.test(host);
    if (nic && !f.interface) f.interface = nic;
    if (many) return { ...f, kind: scheme.toLowerCase() === "rtp" ? "rtp-m" : "udp-m", group: host, port };
    if (scheme.toLowerCase() === "udp" && (!host || host === "0.0.0.0")) return { ...f, kind: "udp-u", port };
    return f;
  }
  m = /^srt:\/\/(@)?(.*)$/i.exec(uri);
  if (m) {
    const address = m[2].replace(/[?#].*$/, "");
    return { ...f, kind: "srt", address, passphrase: str(p.passphrase), latency_ms: p.latency_ms != null ? String(p.latency_ms) : "" };
  }
  for (const k of ["rtmp", "rtsp", "hls"]) if (SCHEME[k].test(uri)) return { ...f, kind: k };
  if (/^file:\/\//i.test(uri)) return { ...f, kind: "file", path: uri.replace(/^file:\/\//i, "") };
  return f;
}

/** A ChannelInput from the fields. `keep` is the params the editor does not show. */
export function inputFrom(f, keep = {}) {
  const asks = ASKS[f.kind] || ASKS.other;
  const params = { ...keep };
  delete params.passphrase;
  delete params.latency_ms;
  delete params.interface;
  if (f.kind === "srt") {
    if (str(f.passphrase)) params.passphrase = str(f.passphrase);
    if (Number(f.latency_ms) > 0) params.latency_ms = Math.round(Number(f.latency_ms));
  }
  const out = { uri: inputUri(f) };
  if (asks.includes("program") && Number(f.program) > 0) out.program = Math.round(Number(f.program));
  if (asks.includes("interface") && str(f.interface)) out.interface = str(f.interface);
  if (Object.keys(params).length) out.params = params;
  return out;
}

/** "239.0.0.1:5000, program 7 on eth1". */
export function inputWords(i) {
  if (!i) return "";
  const f = inputFields(i);
  const where = f.kind === "udp-m" || f.kind === "rtp-m" ? `${f.kind === "rtp-m" ? "RTP " : ""}${f.group}:${f.port}` : i.uri;
  return `${where}${i.program ? `, program ${i.program}` : ""}${f.interface ? ` on ${f.interface}` : ""}${i.backup ? `, backup ${i.backup.uri}` : ""}`;
}

/** Where a channel came from, in one word for the list, and in full. */
export function originOf(entry) {
  const c = entry.channel || {};
  if (entry.follows) return { key: "file", word: "Followed file", full: `A stream of ${entry.follows}, which this station follows.` };
  if (entry.detached_from) return { key: "taken", word: "Taken from the file", full: `Taken over from ${entry.detached_from}. The station keeps it, and the file's edits to it are not applied.` };
  const o = str(c.origin);
  if (/^import/i.test(o)) return { key: "import", word: "Import", full: o };
  return { key: "here", word: "Added here", full: o || "Added here" };
}

/** The note shown before an edit or removal of a followed channel. */
export function takeOverNote(entry, removing) {
  return removing
    ? `${entry.channel.name} is a stream of ${entry.follows}. Removing it here hides it from that file: the file is not changed, and Follow the file again brings it back.`
    : `${entry.channel.name} is a stream of ${entry.follows}. Saving this takes it out of the file's hands: the station keeps it from then on, and later edits to this stream in the file are ignored.`;
}

/** What a profile does, in words. */
export function profileWords(p) {
  if (!p || (!p.video && !p.audio)) return "Passed through as it comes";
  const r = { video: p.video, audio: p.audio };
  const words = renditionWords(r);
  const v = p.video || {};
  const extra = [p.video ? "" : "picture as it comes", v.encoder ? `on the ${v.encoder.toUpperCase()}` : ""].filter(Boolean);
  return [words, ...extra].filter(Boolean).join(" · ");
}

/** The profile a channel takes, by id, or its custom one. */
export function profileOf(setup, channel) {
  if (channel.profile === "custom") return { id: "custom", name: "Custom settings", ...(channel.custom || {}) };
  return ((setup && setup.profiles) || []).find((p) => p.id === channel.profile) || { id: channel.profile, name: channel.profile };
}

// The custom settings, flat, as the form holds them.

export const SIZES = [["source", "As it comes"], ["3840x2160", "3840×2160 (4K)"], ["1920x1080", "1920×1080 (1080p)"], ["1280x720", "1280×720 (720p)"], ["1024x576", "1024×576 (576p)"], ["854x480", "854×480 (480p)"], ["640x360", "640×360 (360p)"]];

/** Fields from a profile's video and audio wants. */
export function convFields(p) {
  const v = (p && p.video) || null;
  const a = (p && p.audio) || null;
  const f = { codec: v ? v.codec || "h264" : "copy", size: "source", fit: "pad", pad_color: "#000000", bitrate_kbps: "", keyframe_s: "2", keyframe_frames: "", deinterlace: "auto", preset: "", bframes: "", encoder: "auto", audio: "copy", audio_kbps: "128", channels: "", tracks: "first" };
  if (v) {
    if (v.width && v.height) f.size = `${v.width}x${v.height}`;
    for (const k of ["fit", "pad_color", "deinterlace", "preset", "encoder"]) if (v[k]) f[k] = v[k];
    if (!v.deinterlace) f.deinterlace = "off";
    if (v.bitrate_kbps) f.bitrate_kbps = String(v.bitrate_kbps);
    f.keyframe_s = v.keyframe_ms ? String(v.keyframe_ms / 1000) : "";
    if (v.keyframe_frames) f.keyframe_frames = String(v.keyframe_frames);
    if (v.bframes != null) f.bframes = String(v.bframes);
  }
  if (a) {
    f.audio = a.codec || "aac";
    if (a.bitrate_kbps) f.audio_kbps = String(a.bitrate_kbps);
    if (a.channels) f.channels = String(a.channels);
    if (a.tracks) f.tracks = a.tracks;
  }
  return f;
}

/** A profile's {video, audio} from the fields. Copy leaves that half out. */
export function wantFrom(f) {
  const out = {};
  const n = (v) => (str(v) !== "" && Number(v) >= 0 ? Math.round(Number(v)) : null);
  if (f.codec && f.codec !== "copy") {
    const v = { codec: f.codec };
    if (f.size && f.size !== "source") {
      const [w, h] = f.size.split("x").map(Number);
      Object.assign(v, { width: w, height: h, fit: f.fit || "pad" });
      if (v.fit === "pad" && /^#[0-9a-f]{6}$/i.test(f.pad_color || "") && f.pad_color.toLowerCase() !== "#000000") v.pad_color = f.pad_color.toLowerCase();
    }
    if (n(f.bitrate_kbps)) v.bitrate_kbps = n(f.bitrate_kbps);
    if (Number(f.keyframe_s) > 0) v.keyframe_ms = Math.round(Number(f.keyframe_s) * 1000);
    if (n(f.keyframe_frames)) v.keyframe_frames = n(f.keyframe_frames);
    if (f.deinterlace && f.deinterlace !== "off") v.deinterlace = f.deinterlace;
    if (str(f.preset)) v.preset = str(f.preset);
    if (n(f.bframes) !== null) v.bframes = n(f.bframes);
    if (f.encoder === "gpu" || f.encoder === "cpu") v.encoder = f.encoder;
    out.video = v;
  }
  if (f.audio && f.audio !== "copy") {
    const a = { codec: f.audio };
    if (n(f.audio_kbps)) a.bitrate_kbps = n(f.audio_kbps);
    if (n(f.channels)) a.channels = n(f.channels);
    if (f.tracks === "all") a.tracks = "all";
    out.audio = a;
  }
  return out;
}

/** A slug from a name, for a new profile's id or a scanned program's channel. */
export function slug(text) {
  return str(text).replace(/\s+/g, "-").replace(/[^A-Za-z0-9._-]/g, "").replace(/-{2,}/g, "-").replace(/^[-.]+|[-.]+$/g, "").slice(0, 64);
}

/**
 * A scanned program's channel name from a pattern: {name} is the SDT
 * name, {number} the program number. A name that comes out empty is
 * P<number>.
 */
export function scanName(pattern, prog) {
  const raw = str(pattern || "{name}").replace(/\{name\}/g, prog.name || "").replace(/\{number\}/g, String(prog.number));
  return slug(raw) || `P${prog.number}`;
}

/** A program's picture and sound in words, for the scan's table. */
export function trackWords(prog) {
  const tracks = prog.tracks || [];
  const vid = tracks.filter((t) => t.kind === "video").map((t) => [t.codec.toUpperCase(), t.width && t.height ? `${t.width}×${t.height}${t.interlaced ? "i" : "p"}` : t.interlaced ? "interlaced" : "", t.fps ? `${Math.round(t.fps * 100) / 100} fps` : ""].filter(Boolean).join(" "));
  const snd = tracks.filter((t) => t.kind === "audio").map((t) => [t.codec.toUpperCase(), t.channels ? (t.channels === 2 ? "stereo" : t.channels === 1 ? "mono" : `${t.channels} ch`) : "", t.lang || ""].filter(Boolean).join(" "));
  if (!tracks.length) return { video: (prog.streams || []).filter((s) => /264|265|hevc|mpeg2|video/i.test(s)).join(", ") || "none seen", sound: (prog.streams || []).filter((s) => /aac|ac3|mp2|mp3|audio|opus/i.test(s)).join(", ") || "none seen" };
  return { video: vid.join(", ") || "no picture", sound: snd.join(", ") || "no sound" };
}

/** A copy of a channel to start a duplicate from. */
export function duplicateOf(channel, names) {
  const c = structuredClone(channel);
  delete c.origin;
  let i = 2;
  const base = c.name.replace(/-\d+$/, "");
  while (names.has(`${base}-${i}`)) i += 1;
  c.name = `${base}-${i}`;
  if (c.title) c.title = `${c.title} (copy)`;
  return c;
}

/** headend.channel.list, or an empty setup where the station has none. */
export async function readSetup(client) {
  try {
    const s = await client.call("headend.channel.list", {});
    return { channels: [], profiles: [], encoders: [], ...s };
  } catch (e) {
    return { channels: [], profiles: [], encoders: [], missing: true, error: e };
  }
}
