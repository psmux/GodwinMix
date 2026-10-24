// A stand in for a headend station's own setup, on top of the wall's stub:
// headend.channel.list, add (with its dry run), update, bulk, remove and
// follow, headend.profile.save and remove (refused with data.channels when
// a channel takes the profile), headend.scan after a short listen, and
// headend.export. Each channel added runs as a direct show in show.list.
// Shapes are crates/godwinmix-protocol/src/headend.

import { wallStub } from "./wall-stub.js";
import { RpcError } from "../client/errors.js";

const SOUND = { codec: "aac", bitrate_kbps: 128, tracks: "all" };
const h264 = (width, height, kbps) => ({ codec: "h264", width, height, fit: "pad", bitrate_kbps: kbps, keyframe_ms: 2000, deinterlace: "auto" });

export const BUILTINS = [
  { id: "copy", name: "Pass through", builtin: true },
  { id: "aac-sound", name: "Picture as it comes, sound to AAC", builtin: true, audio: SOUND },
  { id: "h264-480p-800", name: "480p H.264, 800 kbit/s", builtin: true, video: h264(854, 480, 800), audio: SOUND },
  { id: "h264-720p-1500", name: "720p H.264, 1.5 Mbit/s", builtin: true, video: h264(1280, 720, 1500), audio: SOUND },
  { id: "h264-1080p-4000", name: "1080p H.264, 4 Mbit/s", builtin: true, video: h264(1920, 1080, 4000), audio: SOUND },
];

const HLS = { segments: "both", access: "open", segment_ms: 3400 };
const showId = (name) => name.toLowerCase().replace(/[^a-z0-9]+/g, "-");

/** Three channels: one added here, one a followed file's stream, one taken over from it. */
export function someChannels() {
  return [
    { channel: { name: "News", title: "News HD", enabled: true, input: { uri: "udp://@239.0.0.1:5000", program: 101, interface: "eth1" }, profile: "h264-720p-1500", hls: HLS, outputs: [], origin: "added here" }, show: "news" },
    { channel: { name: "Sport", enabled: true, input: { uri: "srt://192.0.2.10:9000", params: { latency_ms: 200 } }, profile: "copy", hls: HLS, outputs: [{ id: "push-1", uri: "udp://239.0.1.1:5000", copy: true }] }, show: "sport", follows: "/etc/flussonic/flussonic.conf" },
    { channel: { name: "CH101", enabled: false, input: { uri: "udp://@239.0.0.2:5000", program: 3 }, profile: "lobby", hls: HLS, outputs: [], origin: "/etc/flussonic/flussonic.conf, edited here" }, show: "ch101", detached_from: "/etc/flussonic/flussonic.conf" },
  ];
}

export const SCAN = {
  uri: "udp://@239.0.0.9:5000",
  listened_ms: 3100,
  programs: [
    { number: 101, name: "News HD", provider: "Example TV", streams: ["h264", "aac"], tracks: [{ kind: "video", codec: "h264", width: 1920, height: 1080, fps: 25, interlaced: true }, { kind: "audio", codec: "aac", channels: 2, lang: "eng" }], kbps: 6200 },
    { number: 102, name: "", provider: "", streams: ["mpeg2", "mp2", "mp2"], tracks: [{ kind: "video", codec: "mpeg2", width: 720, height: 576, fps: 25, interlaced: true }, { kind: "audio", codec: "mp2", channels: 2, lang: "eng" }, { kind: "audio", codec: "mp2", channels: 2, lang: "fra" }], kbps: 4100 },
    { number: 103, name: "Movies", provider: "Example TV", streams: ["h264", "ac3"], scrambled: true, tracks: [] },
    { number: 104, name: "Sport", provider: "Example TV", streams: ["h264", "aac"], channel: "Sport", tracks: [{ kind: "video", codec: "h264", width: 1280, height: 720, fps: 50 }, { kind: "audio", codec: "aac", channels: 2 }] },
  ],
};

/**
 * @param {{channels?: Array, saved?: Array, gpu?: boolean, n?: number}} opts
 */
export function headendStub(opts = {}) {
  const channels = opts.channels || [];
  const stub = wallStub({ shows: channels.map((e) => showOf(e.channel)), n: opts.n ?? 0 });
  stub.setup = {
    channels,
    profiles: [...BUILTINS, ...(opts.saved || [{ id: "lobby", name: "Lobby 576p", video: h264(1024, 576, 1200), audio: { codec: "aac", bitrate_kbps: 96 } }])],
    encoders: opts.gpu === false ? [{ id: "x264", codec: "h264", on: "cpu" }] : [{ id: "h264-nvidia", codec: "h264", on: "gpu" }, { id: "hevc-nvidia", codec: "h265", on: "gpu" }, { id: "x264", codec: "h264", on: "cpu" }],
    following: channels.some((e) => e.follows) ? "/etc/flussonic/flussonic.conf" : undefined,
  };
  stub.scanMs = opts.scanMs ?? 150;
  const own = methods(stub);
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    if (!own[method]) return inner(method, params);
    stub.calls.push({ method, params: structuredClone(params) });
    return structuredClone(await own[method](params));
  };
  return stub;
}

function showOf(c) {
  return { id: showId(c.name), name: c.title || c.name, state: c.enabled === false ? "stopped" : "starting", compositing: false, input: c.input, outputs: [{ id: "hls", uri: "hls://hls", enabled: true, rendition: null, state: "live" }], health: { state: "ok", alarms: [] } };
}

const change = (done = [], refused = [], dry = false) => ({ done, refused, dry_run: dry, restarted: [] });
const NAME = /^[A-Za-z0-9._-]{1,64}$/;

function methods(stub) {
  const s = stub.setup;
  const entry = (name) => s.channels.find((e) => e.channel.name === name);
  const why = (c, names) => !NAME.test(c.name || "") ? `"${c.name}" is not a channel name: letters, digits, - _ and . only.` : names.has(c.name) ? `there is already a channel ${c.name}. Give this one another name, or change that one.` : !(c.input && c.input.uri) ? "it has no input address." : c.profile !== "custom" && !s.profiles.some((p) => p.id === c.profile) ? `there is no profile ${c.profile}.` : null;
  const each = (names, fn) => {
    const done = [];
    const refused = [];
    names.forEach((name, index) => {
      const e = entry(name);
      if (!e) return refused.push({ index, name, why: `there is no channel ${name}.`, data: {} });
      const no = fn(e);
      if (no) return refused.push({ index, name, why: no, data: {} });
      done.push(name);
    });
    return change(done, refused);
  };
  return {
    "headend.channel.list": () => s,
    "headend.channel.add": ({ channels, dry_run }) => {
      const names = new Set(s.channels.map((e) => e.channel.name));
      const done = [];
      const refused = [];
      channels.forEach((c, index) => {
        const no = why(c, names);
        if (no) return refused.push({ index, name: c.name, why: no, data: {} });
        names.add(c.name);
        done.push(c.name);
        if (dry_run) return;
        s.channels.push({ channel: { ...c, origin: "added here" }, show: showId(c.name) });
        const show = showOf(c);
        stub.shows.push(show);
        stub.event("show.changed", { show });
      });
      return change(done, refused, !!dry_run);
    },
    "headend.channel.update": ({ name, channel }) => {
      const e = entry(name);
      if (!e) throw new RpcError(-32004, `there is no channel ${name}.`, {});
      const names = new Set(s.channels.map((x) => x.channel.name).filter((n) => n !== name));
      const no = why(channel, names);
      if (no) return change([], [{ index: 0, name, why: no, data: {} }]);
      if (e.follows) {
        e.detached_from = e.follows;
        delete e.follows;
      }
      e.channel = { ...channel, origin: e.detached_from ? `${e.detached_from}, edited here` : e.channel.origin };
      return { ...change([name]), restarted: [name] };
    },
    "headend.channel.bulk": (p) => each(p.names, (e) => {
      if (p.profile && p.profile !== "custom" && !s.profiles.some((x) => x.id === p.profile)) return `there is no profile ${p.profile}.`;
      if (p.profile) e.channel.profile = p.profile;
      if (p.custom) e.channel.custom = p.custom;
      if (p.enabled !== undefined) e.channel.enabled = p.enabled;
      if (p.interface !== undefined) e.channel.input.interface = p.interface || undefined;
      return null;
    }),
    "headend.channel.remove": ({ names }) => {
      const got = each(names, () => null);
      s.channels = s.channels.filter((e) => !got.done.includes(e.channel.name));
      stub.setup = s;
      return got;
    },
    "headend.channel.follow": ({ names }) => each(names, (e) => {
      if (!e.detached_from) return `${e.channel.name} was not taken over from the followed file, so it already follows it, or was made here.`;
      e.follows = e.detached_from;
      delete e.detached_from;
      return null;
    }),
    "headend.profile.save": ({ profile }) => {
      const at = s.profiles.findIndex((p) => p.id === profile.id);
      if (at >= 0 && s.profiles[at].builtin) throw new RpcError(-32602, `${profile.id} is built in and cannot be changed. Save it under another id.`, {});
      if (at >= 0) s.profiles[at] = { ...profile };
      else s.profiles.push({ ...profile });
      return { id: profile.id, channels: s.channels.filter((e) => e.channel.profile === profile.id).map((e) => e.channel.name) };
    },
    "headend.profile.remove": ({ id, move_to }) => {
      const p = s.profiles.find((x) => x.id === id && !x.builtin);
      if (!p) throw new RpcError(-32004, `there is no saved profile ${id}.`, {});
      const using = s.channels.filter((e) => e.channel.profile === id).map((e) => e.channel.name);
      if (using.length && !move_to) throw new RpcError(-32001, `${using.join(", ")} takes ${id}. Send move_to with the profile they take instead, or change them first.`, { channels: using });
      if (move_to && !s.profiles.some((x) => x.id === move_to)) throw new RpcError(-32602, `there is no profile ${move_to} to move the channels to.`, { move_to });
      for (const e of s.channels) if (e.channel.profile === id) e.channel.profile = move_to;
      s.profiles = s.profiles.filter((x) => x !== p);
      return { id, channels: using };
    },
    "headend.scan": (p) => new Promise((resolve) => setTimeout(() => resolve({ ...SCAN, uri: p.uri }), stub.scanMs)),
    "headend.export": () => ({ text: s.channels.map((e) => `stream ${e.channel.name} {\n  input ${e.channel.input.uri};\n}`).join("\n"), notes: [] }),
  };
}
