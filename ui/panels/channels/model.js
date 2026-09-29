// What the channels page knows, and the words it says about it. No DOM here,
// so every rule is testable with a plain object.
//
// The shapes are the ones in dev/plans/channels-contract.md.

/** "Sunday Service!" becomes "sunday-service", the way the address will read. */
export function slugify(name) {
  return String(name || "")
    .toLowerCase()
    .normalize("NFKD")
    .replace(/\p{M}/gu, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
}

/** Every channel by id, the RTMP port's addresses, and a little history. */
export class Channels {
  constructor() {
    this.byId = new Map();
    this.rtmp = { port: 1935, urls: [] };
    this.history = new Map();
    this.seenAt = new Map();
  }

  /** The answer to `channel.list`. */
  load(answer) {
    this.byId.clear();
    if (answer && answer.rtmp) this.rtmp = answer.rtmp;
    for (const c of (answer && answer.channels) || []) this.put(c);
  }

  /** One channel as the core last described it, from a call or an event. */
  put(channel, now = Date.now()) {
    if (!channel || !channel.id) return;
    this.byId.set(channel.id, channel);
    for (const s of channel.streams || []) {
      const key = channel.id + "/" + s.name;
      this.seenAt.set(key, now);
      const kbps = streamKbps(s);
      if (s.state !== "live") continue;
      const list = this.history.get(key) || [];
      list.push(kbps);
      if (list.length > 40) list.shift();
      this.history.set(key, list);
    }
  }

  remove(id) {
    this.byId.delete(id);
  }

  /** Live first, then the ones waiting, then the ones switched off; by name within. */
  list() {
    const rank = (c) => (isLive(c) ? 0 : c.enabled ? 1 : 2);
    return [...this.byId.values()].sort((a, b) => rank(a) - rank(b) || String(a.name).localeCompare(String(b.name)));
  }

  samples(channelId, stream) {
    return this.history.get(channelId + "/" + stream) || [];
  }
}

/** Video and audio together, which is what a person means by the bitrate. */
export function streamKbps(s) {
  return ((s.video && s.video.kbps) || 0) + ((s.audio && s.audio.kbps) || 0);
}

export function isLive(channel) {
  return (channel.streams || []).some((s) => s.state === "live");
}

export function liveCount(channel) {
  return (channel.streams || []).filter((s) => s.state === "live").length;
}

/**
 * When a state began, as epoch milliseconds. The contract says `since_ms`
 * without saying which clock, so a value too small to be a date is read as how
 * long ago it began when it was sent.
 */
export function startedAt(sinceMs, receivedAt = Date.now()) {
  const n = Number(sinceMs) || 0;
  return n > 1e12 ? n : receivedAt - n;
}

export function fmtUptime(ms) {
  const s = Math.max(0, Math.floor(ms / 1000));
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const pad = (v) => String(v).padStart(2, "0");
  return h ? `${h}:${pad(m)}:${pad(s % 60)}` : `${m}:${pad(s % 60)}`;
}

export function fmtKbps(kbps) {
  const k = Number(kbps) || 0;
  if (k >= 1000) return (k / 1000).toFixed(k >= 10000 ? 0 : 1) + " Mb/s";
  return Math.round(k) + " kb/s";
}

export function fmtFps(fps) {
  const f = Number(fps) || 0;
  return (Math.round(f * 100) / 100).toString() + " fps";
}

const CODECS = { h264: "H.264", avc: "H.264", avc1: "H.264", h265: "HEVC", hevc: "HEVC", av1: "AV1", vp9: "VP9", aac: "AAC", mp3: "MP3", opus: "Opus", pcm: "PCM" };

export function codecName(codec) {
  const c = String(codec || "").toLowerCase();
  return CODECS[c] || c.toUpperCase();
}

/** The badges a stream row shows: what the picture and sound are. */
export function badges(s) {
  const out = [];
  if (s.video) out.push(codecName(s.video.codec));
  if (s.audio) {
    const rate = s.audio.sample_rate ? ` ${Math.round(s.audio.sample_rate / 1000)}k` : "";
    const ch = s.audio.channels === 1 ? " mono" : s.audio.channels === 2 ? " stereo" : s.audio.channels ? ` ${s.audio.channels}ch` : "";
    out.push(codecName(s.audio.codec) + rate + ch);
  }
  return out;
}

export function resolution(s) {
  return s.video && s.video.width ? `${s.video.width}×${s.video.height}` : "";
}

/** The label of the key that let a stream in, or its hint, or nothing. */
export function keyLabel(channel, keyId) {
  const k = (channel.keys || []).find((x) => x.id === keyId);
  return k ? k.label || `key ending ${k.hint}` : "";
}

/**
 * What OBS asks for, in the two boxes it has: Server, and Stream Key. OBS puts
 * a slash between them, so the key box carries the stream name and, for a
 * channel that reads the key from the query, the key after it.
 */
export function obsFields(channel, secret, base, stream = "main") {
  const server = (base ? base.replace(/\/+$/, "") + "/" + channel.app : channel.publish && channel.publish.server) || "";
  const key = channel.key_mode === "stream" ? secret : `${stream}?psk=${secret}`;
  return { server, key, url: server + "/" + key };
}

/** The addresses an encoder can reach this mixer at, first one first. */
export function bases(model, channel) {
  const urls = (model.rtmp && model.rtmp.urls) || [];
  if (urls.length) return urls;
  const server = (channel.publish && channel.publish.server) || "";
  return server ? [server.slice(0, server.length - channel.app.length - 1)] : [];
}

/** The words under a tile, which say what to do when there is something to do. */
export function tileState(d) {
  if (!d.enabled || d.state === "off") return "Off";
  if (d.has_key === false && d.platform !== "srt" && d.platform !== "custom") return "Needs a key";
  switch (d.state) {
    case "waiting": return "Waits for the stream";
    case "connecting": return "Connecting";
    case "live": return d.kbps ? `Live, ${fmtKbps(d.kbps)}` : "Live";
    case "reconnecting": return d.reconnects ? `Trying again (${d.reconnects})` : "Trying again";
    case "failed": return "Stopped";
    default: return d.state || "";
  }
}

/** Which ring a tile wears. */
export function ringState(d) {
  if (!d.enabled) return "off";
  if (d.has_key === false && d.platform !== "srt" && d.platform !== "custom") return "failed";
  return d.state || "off";
}
