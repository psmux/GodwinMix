// A stand in for a core with renditions, in the shapes of
// dev/plans/wave2-contract.md: rendition.presets, rendition.plan,
// governor.status and governor.calibrate, output.add and output.set that the
// governor can refuse with advice, and the two events. Built on the channel
// stub, so one stub can drive the Outputs panel and the Channels panel.

import { ChannelStub } from "./channels-stub.js";
import { RpcError } from "../client/errors.js";

const fps = (n) => ({ num: n, den: 1 });
const video = (w, h, f, kbps, codec = "h264") => ({ codec, width: w, height: h, fps: fps(f), bitrate_kbps: kbps, keyframe_ms: 2000 });
const aac = { codec: "aac", bitrate_kbps: 160 };
const req = (id, v, container = "flv") => ({ id, container, video: v, audio: aac });
const rung = (h, kbps) => req(`${h}p`, video(Math.round((h * 16) / 9 / 2) * 2, h, 30, kbps), "hls");

export const PRESETS = [
  { id: "youtube-1080p30", title: "YouTube 1080p", group: "Platforms", request: req("youtube-1080p30", video(1920, 1080, 30, 6000)) },
  { id: "youtube-720p30", title: "YouTube 720p", group: "Platforms", request: req("youtube-720p30", video(1280, 720, 30, 3000)) },
  { id: "facebook-720p30", title: "Facebook 720p", group: "Platforms", request: req("facebook-720p30", video(1280, 720, 30, 4000)) },
  { id: "twitch-1080p60", title: "Twitch 1080p60", group: "Platforms", request: req("twitch-1080p60", video(1920, 1080, 60, 6000)), available: false, why: "Needs a GPU encoder, or more CPU than this machine has" },
  { id: "twitch-720p30", title: "Twitch 720p", group: "Platforms", request: req("twitch-720p30", video(1280, 720, 30, 3000)) },
  { id: "audio-only-aac", title: "Sound only", group: "Other", request: { id: "audio-only-aac", container: "flv", no_video: true, audio: { codec: "aac", bitrate_kbps: 128 } } },
  { id: "abr-ladder-4", title: "Four sizes", group: "Viewers", request: rung(1080, 6000), ladder: [rung(1080, 6000), rung(720, 3000), rung(480, 1400), rung(360, 800)] },
  { id: "abr-ladder-3", title: "Three sizes", group: "Viewers", request: rung(720, 3000), ladder: [rung(720, 3000), rung(480, 1400), rung(360, 800)] },
  { id: "copy", title: "Same as the source", group: "Other", request: { id: "copy", container: "flv" } },
];

const gpu = { id: "h264-videotoolbox", codec: "h264", hardware: true, device: "apple-m1" };

/** The programme's plan as the planner would answer it for the outputs in `state`. */
export function programmePlan() {
  return {
    nodes: [
      { id: "decode", kind: "decode", serves: [], reason: { code: "ok", text: "" }, cost: { cpu_millicores: 180 } },
      { id: "enc-1080", kind: "encode", serves: ["youtube", "facebook", "viewers"], encoder: gpu, reason: { code: "preferred", text: "The GPU had room." }, cost: { cpu_millicores: 60, device_millis: 380, device_sessions: 1 } },
      { id: "enc-720", kind: "encode", serves: ["twitch"], encoder: { id: "x264", codec: "h264", hardware: false }, reason: { code: "gpu_full", text: "The GPU is full." }, cost: { cpu_millicores: 1100 } },
      { id: "copy-relay", kind: "copy", serves: ["relay"], reason: { code: "copy", text: "The source matches." }, cost: { egress_kbps: 6000 } },
    ],
    totals: { cpu_millicores: 1340, devices: { "apple-m1": { millis: 380, sessions: 1 } }, egress_kbps: 21400 },
  };
}

export const STATUS = () => ({
  calibrated_at: new Date(Date.now() - 3 * 86400 * 1000).toISOString(),
  fingerprint: "m1-8c-16g",
  cpu: { cores: 8, used_millicores: 3400, room_millicores: 2900 },
  devices: [{ id: "apple-m1", kind: "gpu", used_millis: 620, room_millis: 280, sessions_used: 3, sessions_max: 4 }],
  egress_kbps: 21400,
  shed: [{ what: "Multiview thumbnails", why: "The CPU ran short while on air, so the previews stopped first." }],
});

/** The refusal the governor sends for a 1080p encode on a full machine. */
export function refusal(id) {
  return new RpcError(-32003, "Not enough room on this machine for that format without dropping frames on air.", {
    need: { cpu_millicores: 1800, device_millis: 0, device_sessions: 1, memory_mib: 60, egress_kbps: 6000 },
    have: { cpu_millicores: 600, device_millis: 0, device_sessions: 0, memory_mib: 900, egress_kbps: 30000 },
    advice: [
      { text: "720p30 fits", request: req(id, video(1280, 720, 30, 3000)) },
      { text: "Same as the source", request: { id, container: "flv" } },
    ],
  });
}

/**
 * A stub with renditions. `opts.presets: false` is a core without them.
 * `stub.full = true` makes the governor refuse any 1080p request.
 */
export function renditionStub(opts = {}) {
  const stub = new ChannelStub();
  stub.rcalls = [];
  stub.full = false;
  stub.plans = { programme: programmePlan() };
  stub.status = STATUS();
  const renderers = new Set();
  stub.onRender = (fn) => (renderers.add(fn), () => renderers.delete(fn));
  stub.setOutputs = (outputs) => {
    stub.state = { ...stub.state, outputs };
    for (const fn of renderers) fn(stub.state);
  };
  stub.refreshOutputs = async () => {};
  stub.transport = { base: "http://192.168.1.20:8080/" };
  const base = stub.call.bind(stub);
  const own = {
    "core.api": () => ({ kinds: { output: [{ id: "rtmp/output" }, { id: "srt/output" }, { id: "hls/output" }] } }),
    "rendition.presets": () => ({ presets: PRESETS }),
    "rendition.plan": (p) => stub.plans[p.scope || "programme"] || { nodes: [], totals: {} },
    "governor.status": () => stub.status,
    "governor.calibrate": () => ({ started: true }),
    "config.get": () => ({ keys: [["canvas.width", 1920], ["canvas.height", 1080], ["canvas.fps", 30], ["program.video_bitrate_kbps", opts.programmeKbps || 4500]].map(([key, value]) => ({ key, value })) }),
    "output.add": (p) => admit(stub, p),
    "output.set": (p) => admit(stub, p),
  };
  stub.call = async (method, params = {}) => {
    if (own[method]) {
      stub.rcalls.push({ method, params });
      if (opts.presets === false && /^(rendition|governor)\./.test(method)) throw Object.assign(new Error(`no method ${method}`), { code: -32601, data: {} });
      return structuredClone(own[method](params));
    }
    if (/^channel\.destination\.(add|set)$/.test(method)) admit(stub, params);
    return base(method, params);
  };
  stub.event = (name, params) => stub.emit("event", { name, params });
  return stub;
}

/** The governor's answer to an add or a set. */
function admit(stub, p) {
  const r = p.rendition || {};
  const tall = r.preset ? /1080/.test(r.preset) : ((r.video || {}).height || 0) >= 1080;
  if (stub.full && tall) throw refusal(p.id || p.destination || "out");
  return { id: p.id };
}
