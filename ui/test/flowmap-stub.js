// A station for the flow map, built on wall-stub.js: `n` channels taken six
// at a time off multicast groups (one group dead, one with errors), each
// with an HLS output, some converted on a GPU and some sent on by RTMP, SRT
// or UDP; one mixed show this page is on, with cameras, a stream, a file, a
// browser source and a graphic, a program, two renditions and three
// outputs; and gpu.status and machine.status. Every call is counted.

import { wallStub } from "./wall-stub.js";

const GPU = [{ kind: "decode", element: "nvh264dec", gpu: true, accel: "nvidia" }, { kind: "scale", element: "cudaconvertscale", gpu: true }, { kind: "encode", element: "nvh264enc", gpu: true, accel: "nvidia" }];

function channels(n) {
  return Array.from({ length: n }, (_, i) => {
    const g = Math.floor(i / 6);
    const id = `ch${101 + i}`;
    const outputs = [{ id: "hls", platform: "hls", uri_host: "hls://tv", enabled: true, state: "live", rendition: i % 3 ? { video: { width: 1280, height: 720, bitrate_kbps: 1500 } } : null,
      plan: i % 3 ? { mode: "transcode", encoder: "h264-nvenc", video: { codec: "h264", width: 1280, height: 720, fps: { num: 25, den: 1 }, bitrate_kbps: 1500 } } : null,
      playback: { master_url_path: `/hls/hls/master.m3u8?show=${id}`, viewers: (i * 7) % 5, segments: i % 2 ? "both" : "cmaf", access: i % 4 ? "keyed" : "open", segment_ms: 3400 } }];
    if (i % 10 === 4) outputs.push({ id: "relay", platform: "custom", uri_host: "rtmp://cdn.example.net:1935", enabled: true, state: i === 14 ? "reconnecting" : "live" });
    if (i % 10 === 7) outputs.push({ id: "srt", platform: "srt", uri_host: "srt://203.0.113.20:9000", enabled: true, state: "live" });
    if (i % 25 === 12) outputs.push({ id: "lan", platform: "udp", uri_host: "udp://239.20.0.1:6000", enabled: true, state: "live" });
    const stopped = i === n - 2;
    return { id, name: `CH${101 + i}`, state: stopped ? "stopped" : "running", compositing: false, input: { uri: `udp://@239.10.0.${g + 1}:${5001 + g}`, program: (i % 6) + 1 }, outputs, health: { state: stopped ? "off" : "ok", alarms: [] } };
  });
}

function statsOf(show, i, t, dead, lossy) {
  const off = show.state === "stopped" || dead;
  const k = off ? 0 : Math.round([2400, 3800, 6000, 1900][i % 4] * (0.9 + 0.1 * Math.sin(t / 3 + i)));
  const cc = lossy ? Math.floor(t * (1 + (i % 3))) : 0;
  return {
    id: show.id, health: show.health, work: show.outputs.some((o) => o.rendition) ? "transcode" : "copy",
    input: off ? { kbps: 0 } : { kbps: k, fps: i % 5 === 0 ? 50 : 25, width: 1920, height: 1080, video_codec: i % 2 ? "mpeg2" : "h264", audio_codec: i === 9 ? undefined : "ac3", audio_channels: 2, cc_errors: cc, packets_lost: 0, keyframe_ms: 2000 },
    outputs: show.outputs.map((o) => ({ id: o.id, state: off ? "waiting" : o.state, kbps: off ? 0 : o.rendition ? 1620 : k, reconnects: o.state === "reconnecting" ? 3 : 0, rendition_text: o.rendition ? "h264 1280x720 25 fps on nvh264enc" : "copy", encoder: o.rendition ? "h264-nvenc" : null, cpu_millicores: o.rendition ? 30 : 0, steps: o.rendition ? GPU : undefined })),
  };
}

const src = (id, name, type, uri, state = "live") => ({ id, name, type, uri, state, has_video: type !== "audio-device", has_audio: true, video_idle_ms: 20 });

/** The page's own show: what client.state holds for a mixer. */
function mixer(stub) {
  Object.assign(stub.state, {
    connected: true, program: null, scene: "Interview", sceneName: "Interview", preview: "Wide",
    sources: [src("cam1", "Camera 1", "camera", "v4l2:///dev/video0"), src("cam2", "Camera 2", "camera", "v4l2:///dev/video2"), src("guest", "Guest", "srt", "srt://0.0.0.0:9000"),
      src("clip", "Opener", "file", "file:///media/opener.mp4"), src("web", "Scores", "browser", "https://scores.example/overlay"), src("lower", "Lower third", "graphic/lower-third", "graphic:lower", "connecting")],
    tally: { cam1: "program", lower: "program", cam2: "preview" },
    outputs: [{ id: "youtube", uri_host: "rtmp://a.rtmp.youtube.com", state: "live", reconnects: 0, bytes_out: 0 }, { id: "recording", uri_host: "file:///recordings", state: "live", bytes_out: 0 }, { id: "backup", uri_host: "srt://203.0.113.9:7001", state: "reconnecting", reconnects: 2, bytes_out: 0 }],
    multiview: { enabled: true, width: 1280, height: 720, cols: 3, rows: 3, cells: [], fps: 25 },
    backend: { video_decoder: "avdec_h264", video_encoder: "nvh264enc", audio_decoder: "avdec_aac", audio_encoder: "avenc_aac", hardware_accelerated: true }, uptime_secs: 5400,
  });
  setInterval(() => { for (const o of stub.state.outputs) o.bytes_out += o.state === "live" ? (o.id === "youtube" ? 6200 : 9000) * 125 : 0; }, 1000);
}

export function flowStub(opts = {}) {
  const n = opts.n ?? 48;
  const main = { id: "main", name: "Main", state: "running", compositing: true, programme_kbps: 15200, cpu_millicores: 1450, memory_mib: 420, health: { state: "ok", alarms: [] } };
  const shows = opts.mixedOnly ? [main] : [...(opts.noMixer ? [] : [main]), ...channels(n)];
  const stub = wallStub({ shows });
  mixer(stub);
  const deadGroup = Math.min(3, Math.floor(n / 6) - 1), lossyGroup = 1;
  stub.cut = opts.cut || null;
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    const t = Date.now() / 1000;
    if (method === "show.list") return structuredClone({ shows: stub.shows, current: shows[0] && shows[0].id });
    if (method === "show.stats") {
      stub.calls.push({ method, params });
      const list = stub.shows.filter((s) => s.compositing === false).map((s, i) => statsOf(s, i, t, Math.floor(i / 6) === deadGroup || stub.cut === s.id, Math.floor(i / 6) === lossyGroup));
      return { shows: list.filter((s) => !params.ids || params.ids.includes(s.id)) };
    }
    if (method === "gpu.status") return stub.calls.push({ method, params }), gpuStatus(t);
    if (method === "machine.status") return stub.calls.push({ method, params }), { cpu_pct: 31, cores: 64, memory_total_mib: 128000, memory_available_mib: 90000, uptime_secs: 86400 * 3 };
    if (method === "rendition.plan") return stub.calls.push({ method, params }), plan();
    return inner(method, params);
  };
  return stub;
}

function gpuStatus(t) {
  const w = (p, lo, hi) => Math.round(lo + (hi - lo) * (0.5 + 0.5 * Math.sin((t * 2 * Math.PI) / p)));
  return { gpus: [{ index: 0, name: "NVIDIA GeForce RTX 3060", driver: "550.144", now: { at_ms: Date.now(), encoder_pct: w(37, 70, 88), decoder_pct: w(23, 38, 52), gpu_pct: w(51, 22, 41), memory_used_mib: 6100, memory_total_mib: 12288, encoder_sessions: 33, temperature_c: 64, power_w: 108 }, history: [] }] };
}

const plan = () => ({ nodes: [
  { id: "encode:programme:h264:1920x1080", kind: "encode", serves: ["youtube", "recording"], encoder: { id: "h264-nvenc", codec: "h264", hardware: true }, cost: {} },
  { id: "encode:programme:h264:1280x720", kind: "encode", serves: ["backup"], encoder: { id: "x264", codec: "h264", hardware: false }, cost: {} },
], totals: { cpu_millicores: 900, devices: {}, egress_kbps: 15000 } });
