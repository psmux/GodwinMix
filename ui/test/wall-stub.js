// A stand in for a station of many shows, in the shapes of
// dev/plans/wave4-contract.md: show.list with compositing, input, outputs
// and health; show.stats; show.add_many with its dry run; show.set;
// show.output.add, set and remove; governor.status; event/show.health and
// event/show.changed. Built on the renditions stub, so presets and the
// format step work against it too. Every show.stats and every picture asked
// for is counted, which is what the test of 200 rows reads.

import { renditionStub, STATUS } from "./renditions-stub.js";
import { RpcError } from "../client/errors.js";
import { picture } from "./wall-picture.js";

const WORDS = ["News", "Sport", "Movies", "Kids", "Music", "Weather", "Docs", "Drama", "Comedy", "Food", "Travel", "Science", "History", "Nature", "Cars", "Fashion", "Shop", "Arts", "Faith", "Gaming"];
const CODEC = ["h264", "h264", "h264", "mpeg2", "hevc"];
const ALARM_AT = { 3: ["black"], 11: ["freeze", "silence"], 17: ["output-failed"], 26: ["stall"], 40: ["no-input"], 58: ["silence"] };
const WARN_AT = { 7: ["cc-errors"], 33: ["loss"], 71: ["cc-errors"] };

const health = (i, now) => {
  const kinds = ALARM_AT[i] || WARN_AT[i];
  if (!kinds) return { state: i % 37 === 36 ? "off" : "ok", alarms: [] };
  return { state: ALARM_AT[i] ? "alarm" : "warning", alarms: kinds.map((kind, k) => ({ kind, since_ms: now - (i * 47 + k * 300) * 1000, detail: `${kind} on the main input` })) };
};

/** `n` direct shows off a headend, a few in alarm, one stopped every 37th. */
export function headend(n = 200, now = Date.now()) {
  return Array.from({ length: n }, (_, i) => {
    const name = `${WORDS[i % WORDS.length]} ${Math.floor(i / WORDS.length) + 1}`;
    const id = name.toLowerCase().replace(/\s+/g, "-");
    const h = health(i, now);
    const group = 1 + Math.floor(i / 250);
    const uri = i % 13 === 5 ? `srt://10.20.0.${i % 250}:9000` : i % 29 === 9 ? `rtmp://ingest.local/live/${id}` : `udp://@239.${group}.${Math.floor(i / 250)}.${i % 250}:5000`;
    const outputs = [{ id: "udp-out", uri: `udp://10.0.0.50:${6000 + i * 2}`, enabled: true, rendition: null, state: h.alarms.some((a) => a.kind === "output-failed") ? "failed" : h.state === "off" ? "stopped" : "live" }];
    if (i % 4 === 1) outputs.push({ id: "youtube", platform: "youtube", enabled: true, rendition: { preset: "youtube-720p30" }, state: "live" });
    if (i % 9 === 2) outputs.push({ id: "srt-backup", uri: "srt://backup.example:7001", enabled: true, rendition: null, state: "reconnecting" });
    return { id, name, state: h.state === "off" ? "stopped" : "running", on_air: null, compositing: false, input: { uri, program: i % 5 === 0 ? 1 : undefined }, outputs, health: h, alarms: { enabled: true, black_ms: 5000, freeze_ms: 10000, silence_ms: 10000, silence_dbfs: -60 } };
  });
}

function stats(show, i, t) {
  const off = show.state === "stopped" || show.health.alarms.some((a) => a.kind === "no-input");
  const base = [3200, 6100, 8000, 4500, 2400][i % 5];
  const k = off ? 0 : Math.round(base * (0.92 + 0.08 * Math.sin(t / 3 + i)));
  const tall = i % 3 === 0;
  return {
    id: show.id,
    health: show.health,
    input: off ? { kbps: 0 } : { kbps: k, fps: i % 7 === 0 ? 50 : 25, width: tall ? 1920 : 1280, height: tall ? 1080 : 720, video_codec: CODEC[i % 5], audio_codec: i % 4 ? "aac" : "mp2", audio_channels: 2, cc_errors: show.health.alarms.some((a) => a.kind === "cc-errors") ? 14 : 0, packets_lost: show.health.alarms.some((a) => a.kind === "loss") ? 37 : 0, keyframe_ms: 1000, last_frame_ms: 40 },
    outputs: show.outputs.map((o) => ({ id: o.id, state: o.state, kbps: o.state === "live" ? (o.rendition ? 3000 : k) : 0, reconnects: o.state === "reconnecting" ? 4 : 0, rendition_text: o.rendition ? "720p30" : "Copy", encoder: o.rendition ? "h264-videotoolbox" : null, cpu_millicores: o.rendition ? 90 : 4 })),
  };
}

/**
 * @param {{shows?: Array, n?: number, noShows?: boolean}} opts
 */
export function wallStub(opts = {}) {
  const stub = renditionStub();
  stub.shows = opts.shows || headend(opts.n ?? 200);
  stub.statsAsked = [];
  stub.thumbAsked = [];
  stub.tasks = { n: 0 };
  stub.switchMs = opts.switchMs ?? 400;
  stub.status = { ...STATUS(), ingress_kbps: 0 };
  const pics = new Map();
  stub.showThumbUrl = (id) => {
    stub.thumbAsked.push(id);
    const i = stub.shows.findIndex((s) => s.id === id);
    if (!pics.has(id) && i >= 0) pics.set(id, picture(stub.shows[i], i));
    return pics.get(id) || "data:,";
  };
  const own = methods(stub, opts);
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    if (!own[method]) return inner(method, params);
    stub.calls.push({ method, params });
    if (opts.noShows) throw new RpcError(-32601, `no method ${method}`, {});
    return structuredClone(own[method](params));
  };
  stub.event = (name, params) => stub.emit("event", { name, params });
  return stub;
}

export function methods(stub) {
  const find = (id) => stub.shows.find((s) => s.id === id) || (() => { throw new RpcError(-32602, `There is no show "${id}". show.list says which there are.`, { id }); })();
  const changed = (s) => stub.event("show.changed", { show: s });
  return {
    "show.list": () => ({ shows: stub.shows, current: stub.shows[0] && stub.shows[0].id }),
    "show.stats": ({ ids }) => {
      stub.statsAsked.push(ids || null);
      const t = Date.now() / 1000;
      return { shows: stub.shows.map((s, i) => [s, i]).filter(([s]) => !ids || ids.includes(s.id)).map(([s, i]) => stats(s, i, t)) };
    },
    "governor.status": () => ({ ...stub.status, ingress_kbps: stub.shows.length * 4800, egress_kbps: stub.shows.length * 5600 }),
    "show.set": (p) => {
      const s = find(p.id);
      if (p.compositing === false && s.scenes_in_use) throw new RpcError(-32001, `${s.name} uses ${s.scenes_in_use} scenes, so it cannot go direct. Take its scenes off air and keep one source, then switch.`, { id: s.id, scenes: s.scenes_in_use });
      const switching = "compositing" in p && p.compositing !== (s.compositing !== false);
      for (const k of ["name", "input", "alarms"]) if (k in p) s[k] = p[k];
      if (!switching) return changed(s), s;
      // A switch answers at once with a task, as the station's does, and
      // lands after `switchMs`.
      const task_id = `show-set-${++stub.tasks.n}`;
      stub.tasks[task_id] = { task_id, kind: "show.set", state: "running", poll_interval_ms: 250 };
      setTimeout(() => {
        s.compositing = p.compositing;
        changed(s);
        stub.tasks[task_id] = { task_id, kind: "show.set", state: "completed", result: { ...s, switch: { compositing: p.compositing, outputs: s.outputs.map((o) => o.id), gap_ms: 900 } } };
      }, stub.switchMs);
      return { task_id, poll_interval_ms: 250, outcome: "indeterminate", state: "running", show: { ...s }, switching: p.compositing ? "on" : "off" };
    },
    "task.get": (p) => stub.tasks[p.task_id] || (() => { throw new RpcError(-32004, `There is no task "${p.task_id}".`, { kind: "task" }); })(),
    // `id` is the show and `output` the output, as channel.destination.* name them; `show` is another name for `id`.
    "show.output.add": (p) => { const s = find(p.id || p.show); const o = { id: p.output || `out-${s.outputs.length + 1}`, uri: p.uri, platform: p.platform, enabled: true, rendition: p.rendition ?? null, state: "connecting" }; s.outputs.push(o); changed(s); return o; },
    "show.output.set": (p) => { const s = find(p.id || p.show); const o = s.outputs.find((x) => x.id === p.output); for (const k of ["uri", "enabled", "rendition"]) if (k in p) o[k] = p[k]; changed(s); return o; },
    "show.output.remove": (p) => { const s = find(p.id || p.show); s.outputs = s.outputs.filter((x) => x.id !== p.output); changed(s); return { removed: p.output }; },
    "show.add_many": (p) => addMany(stub, p),
  };
}

const KNOWN = /^(udp|srt|rtmps?|rtsp|rist|https?|file|channel):/i;

/** Validated whole first; with dry_run, only what it would do. */
function addMany(stub, { shows, dry_run }) {
  const refused = [];
  const ok = [];
  const names = new Set(stub.shows.map((s) => s.name.toLowerCase()));
  let cpu = 0;
  shows.forEach((s, index) => {
    const uri = s.input && s.input.uri;
    const why = !s.name ? "It has no name. Give it one." : names.has(s.name.toLowerCase()) ? `There is already a show called ${s.name}. Pick another name.` : !uri ? "It has no input. Put the feed's address in." : !KNOWN.test(uri) ? `"${uri}" is not an address this station reads. Start it with udp://, srt://, rtmp://, rtsp:// or https://.` : null;
    if (why) return refused.push({ index, name: s.name, why, data: { field: !s.name ? "name" : names.has((s.name || "").toLowerCase()) ? "name" : "input" } });
    names.add(s.name.toLowerCase());
    cpu += (s.outputs || []).filter((o) => o.rendition).length * 900;
    ok.push(s);
  });
  const room = stub.status.cpu.room_millicores;
  const plan = { cost: { cpu_millicores: cpu, egress_kbps: ok.reduce((n, s) => n + (s.outputs || []).length * 5000, 0), memory_mib: ok.length * 6 }, fits: cpu <= room, room: { cpu_millicores: room } };
  if (dry_run) return { added: ok.map((s) => s.name.toLowerCase().replace(/\W+/g, "-")), refused, plan };
  const added = ok.map((s) => {
    const show = { id: s.name.toLowerCase().replace(/\W+/g, "-"), name: s.name, state: "starting", compositing: !!s.compositing, input: s.input, outputs: (s.outputs || []).map((o, i) => ({ id: o.id || `out-${i + 1}`, enabled: true, state: "connecting", ...o })), health: { state: "ok", alarms: [] } };
    stub.shows.push(show);
    stub.event("show.changed", { show });
    return show.id;
  });
  return { added, refused, plan };
}
