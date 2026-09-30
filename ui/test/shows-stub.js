// A stand in for a station with several shows, in the shapes of
// dev/plans/wave3-contract.md: show.list, show.add, show.rename, show.remove,
// show.start and show.stop, event/show.changed and event/show.removed, and
// each show's own output.list, source.list and rendition.plan behind
// `linkShow(id)`, which is how the routing view reaches a show that is not
// the page's. Built on the renditions stub, so channels and their plans are
// there too.

import { renditionStub } from "./renditions-stub.js";
import { liveStream } from "./channels-stub.js";

const out = (id, host, extra = {}) => ({ id, type: "rtmp/output", uri_host: host, has_key: true, state: "live", reconnects: 0, ...extra });
const src = (id, name) => ({ id, name, type: "rtmp/source", state: "live" });
const enc = (serves, encoder, cpu, extra = {}) => ({ kind: "encode", serves, encoder, cost: { cpu_millicores: cpu, ...extra } });

/** Three shows as a church on a Sunday runs them. */
export function sundayShows() {
  return [
    { id: "main", name: "Sunday service", state: "running", on_air: "Worship", programme_kbps: 6200, cpu_millicores: 1800 },
    { id: "kids", name: "Kids room", state: "running", on_air: null, programme_kbps: 3000, cpu_millicores: 700 },
    { id: "loop", name: "Overnight loop", state: "stopped", on_air: null, programme_kbps: 0, cpu_millicores: 0 },
  ];
}

/** What each show has inside it, as its own socket would answer. */
export function sundayDetail() {
  return {
    main: {
      sources: [src("cam-wide", "Wide camera"), src("cam-pulpit", "Pulpit camera"), src("lyrics", "Lyrics"), src("sunday-service-main", "Channel: main")],
      tally: { "cam-wide": "program", lyrics: "program" },
      outputs: [out("youtube", "rtmp://a.rtmp.youtube.com/…"), out("facebook", "rtmps://live-api-s.facebook.com/…", { rendition: { preset: "facebook-720p30" } }), out("record", "", { type: "record/output" })],
      plan: { nodes: [enc(["facebook"], { id: "h264-videotoolbox", hardware: true }, 60, { device_millis: 180 })] },
    },
    kids: {
      sources: [src("kids-cam", "Kids camera"), src("slides", "Slides")],
      tally: {},
      outputs: [out("hall-screen", "srt://10.0.0.40:9000", { type: "srt/output", rendition: { video: { width: 1280, height: 720, bitrate_kbps: 2500 } } })],
      plan: { nodes: [enc(["hall-screen"], "x264", 1100)] },
    },
    loop: { sources: [], tally: {}, outputs: [], plan: { nodes: [] } },
  };
}

/**
 * @param {{shows?: Array, current?: string, detail?: object, noShows?: boolean}} opts
 * `noShows` is a core from before shows: every show.* is "no such method".
 */
export function showStub(opts = {}) {
  const stub = renditionStub();
  stub.shows = structuredClone(opts.shows || sundayShows());
  stub.current = opts.current || stub.shows[0].id;
  stub.detail = structuredClone(opts.detail || sundayDetail());
  stub.links = [];
  let n = 0;
  const find = (id) => {
    const s = stub.shows.find((x) => x.id === id);
    if (!s) throw Object.assign(new Error(`There is no show "${id}". show.list says which there are.`), { code: -32602, data: { id, shows: stub.shows.map((x) => x.id) } });
    return s;
  };
  const changed = (s) => stub.emit("event", { name: "show.changed", params: { show: s } });
  const own = {
    "show.list": () => ({ shows: stub.shows, current: stub.current }),
    "show.add": ({ name, from }) => {
      const s = { id: `show-${++n}`, name, state: "starting", on_air: null, programme_kbps: 0, cpu_millicores: 0 };
      stub.shows.push(s);
      stub.detail[s.id] = { sources: [], tally: {}, outputs: [], plan: { nodes: [] }, from };
      changed(s);
      return s;
    },
    "show.rename": ({ id, name }) => { const s = find(id); s.name = name; changed(s); return s; },
    "show.remove": ({ id }) => {
      find(id);
      if (stub.shows.length === 1) throw Object.assign(new Error("That is the only show; a station keeps one. Add another show before removing this one."), { code: -32001, data: { id, action: { method: "show.add" } } });
      stub.shows = stub.shows.filter((s) => s.id !== id);
      if (stub.current === id) stub.current = stub.shows[0].id;
      stub.emit("event", { name: "show.removed", params: { id } });
      return { removed: id };
    },
    "show.start": ({ id }) => { const s = find(id); s.state = "running"; changed(s); return s; },
    "show.stop": ({ id }) => { const s = find(id); s.state = "stopped"; s.on_air = null; changed(s); return s; },
  };
  const inner = stub.call;
  stub.call = async (method, params = {}) => {
    if (own[method]) {
      stub.calls.push({ method, params });
      if (opts.noShows) throw Object.assign(new Error(`no method ${method}`), { code: -32601, data: {} });
      return structuredClone(own[method](params));
    }
    const d = stub.detail[stub.current];
    if (method === "output.list" && d) return structuredClone(d.outputs);
    if (method === "source.list" && d) return structuredClone(d.sources);
    if (method === "rendition.plan" && !params.scope && d) return structuredClone(d.plan);
    return inner(method, params);
  };
  stub.state = { ...stub.state, tally: (stub.detail[stub.current] || {}).tally || {} };
  stub.linkShow = (id) => {
    const link = showLinkStub(stub, id);
    stub.links.push(link);
    return link;
  };
  return stub;
}

/** One show's own socket: its outputs, sources and plan. */
function showLinkStub(stub, id) {
  const fns = new Set();
  const link = {
    id,
    closed: false,
    calls: [],
    async call(method, params = {}) {
      link.calls.push({ method, params });
      const d = stub.detail[id];
      if (method === "output.list") return structuredClone(d.outputs);
      if (method === "source.list") return structuredClone(d.sources);
      if (method === "rendition.plan") return structuredClone(d.plan);
      if (method === "output.add") { d.outputs.push(out(params.id, params.uri)); return { id: params.id }; }
      return stub.call(method, params);
    },
    on: (fn) => (fns.add(fn), () => fns.delete(fn)),
    close() { link.closed = true; },
    event: (name, params) => { for (const fn of fns) fn(name, params); },
  };
  link.tally = stub.detail[id].tally;
  link.client = { call: link.call, on: () => () => {}, listen: () => () => {}, state: { outputs: [] }, refreshOutputs: async () => {} };
  return link;
}

/** Three channels: two live streams on one, one on another, one waiting. */
export async function sundayChannels(stub) {
  const add = async (name) => (await stub.call("channel.add", { name })).channel.id;
  const sunday = await add("Sunday service");
  const youth = await add("Youth room");
  await add("Radio");
  stub.channels.get(sunday).streams = [liveStream("main", { protocol: "srt", since_ms: Date.now() - 900000 }), liveStream("backup", { protocol: "rtmp", since_ms: Date.now() - 60000, video: { codec: "h264", width: 1280, height: 720, fps: 30, kbps: 2500 } })];
  stub.channels.get(youth).streams = [liveStream("stage", { protocol: "whip", video: { codec: "h264", width: 1280, height: 720, fps: 30, kbps: 3000 } })];
  for (const d of [
    { platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "k" },
    { platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "k", rendition: { preset: "facebook-720p30" } },
    { platform: "twitch", server: "rtmp://live.twitch.tv/app", key: "k", rendition: { preset: "twitch-720p30" } },
    { platform: "custom", label: "Backup server", server: "rtmp://10.0.0.9/live/backup", stream: "backup" },
  ]) await stub.call("channel.destination.add", { id: sunday, ...d });
  await stub.call("channel.destination.add", { id: youth, platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "k" });
  stub.plans[`channel:${sunday}`] = { nodes: [
    { kind: "copy", serves: ["youtube", "backup-server"] },
    enc(["facebook"], { id: "h264-videotoolbox", hardware: true }, 70, { device_millis: 200 }),
    enc(["twitch"], "x264", 1200),
  ] };
  return { sunday, youth };
}
