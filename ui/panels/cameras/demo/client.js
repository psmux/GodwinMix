// A pretend mixer for the camera window: three PTZ cameras on one stage
// with people walking about, two ATEM switchers, and GodwinMix's own
// programme. It has the shape of the real client (call, on, onRender,
// listen), so the panel and api.js cannot tell the difference, and it
// counts every call so a test can read what was sent. Its camera tools
// answer in the ptz plugin's own shapes, so the demo goes through the same
// translation in shapes.js as a real camera does.
//
// The clock runs only between start() and stop(), which the panel calls
// while it is on screen. A test passes {clock: false} and steps it by hand.

import { TOOLS } from "../api.js";
import { world, stepWorld } from "./world.js";
import { camera, stepCamera } from "./rig.js";
import { paintCamera } from "./paint.js";
import { handlers } from "./tools.js";
import { switcher, stepSwitcher, sourceOn } from "./atem.js";

const BY_NAME = Object.fromEntries(Object.entries(TOOLS).map(([k, v]) => [v, k]));

function rigs() {
  return [
    camera("cam-wide", "Wide", { cx: 0.5, cy: 0.5, w: 1 }),
    camera("cam-centre", "Centre", { cx: 0.5, cy: 0.62, w: 0.4 }, { tint: "rgba(255,190,120,0.05)" }),
    camera("cam-side", "Side", { cx: 0.5, cy: 0.62, w: 0.45 }, { mirror: true, tint: "rgba(120,170,255,0.06)" }),
  ];
}

export function demoClient(opts = {}) {
  let t = 0;
  const listeners = new Map();
  const d = { world: world(opts.seed || 7), cams: rigs(), now: () => t * 1000, calls: [] };
  d.atems = [switcher("atem-main", "Main switcher", "ATEM Television Studio HD8", 8, d.cams, "follow")];
  if (opts.atems !== 1) d.atems.push(switcher("atem-second", "Second room", "ATEM Mini Pro", 4, d.cams, "own"));
  if (opts.atems === 0) d.atems = [];
  const state = { connected: true, program: "cam-wide", tally: {}, keys: [] };
  const emit = (name, arg) => (listeners.get(name) || new Set()).forEach((fn) => fn(arg));
  const tally = () => {
    state.tally = {};
    if (state.program) state.tally[state.program] = "program";
    emit("render", state);
  };
  d.thumbnail = (c) => {
    const cv = document.createElement("canvas");
    cv.width = 192;
    cv.height = 108;
    paintCamera(cv.getContext("2d"), 192, 108, d.world, c);
    return cv.toDataURL("image/jpeg", 0.7);
  };
  const tools = handlers(d);
  let lastLead = null;

  function step(dt) {
    t += dt;
    stepWorld(d.world, dt);
    for (const c of d.cams) stepCamera(c, d.world, dt, t * 1000);
    for (const s of d.atems) stepSwitcher(s, dt);
    const lead = d.atems.find((s) => s.mode === "lead");
    const src = lead && sourceOn(lead, "program");
    if (src && src !== lastLead && src !== state.program) (state.program = src), tally();
    lastLead = src;
  }

  function core(method, p) {
    if (method === "program.take") {
      state.program = p.source || null;
      for (const s of d.atems) if (s.mode === "follow" && s.map[p.source]) s.mes[0].program = s.map[p.source];
      tally();
      return { source: state.program };
    }
    if (method === "program.key.set") {
      state.keys = state.keys.filter((k) => k !== p.scene).concat(p.on ? [p.scene] : []);
      return { keys: state.keys };
    }
    throw new Error(`the demo mixer does not answer ${method}`);
  }

  let timer = null;
  const client = {
    demo: d,
    state,
    calls: d.calls,
    async call(method, params = {}) {
      d.calls.push({ method, params });
      if (method !== "tool.call") return core(method, params);
      const key = BY_NAME[params.name];
      if (!key) throw new Error(`no plugin has a tool called '${params.name}'`);
      // The core passes a plugin's answer on as it came: MCP's shape, snake case.
      const facts = tools[key](params.arguments || {});
      return { content: [{ type: "text", text: `${params.name} answered.` }], structured_content: facts };
    },
    on(name, fn) {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(fn);
      return () => listeners.get(name).delete(fn);
    },
    onRender(fn) {
      fn(state);
      return client.on("render", fn);
    },
    listen: () => () => {},
    snapshotUrl: () => "",
    paint: (id, canvas) => {
      const c = d.cams.find((x) => x.id === id);
      if (c) paintCamera(canvas.getContext("2d"), canvas.width, canvas.height, d.world, c);
    },
    step,
    start() {
      if (timer || opts.clock === false) return;
      let last = performance.now();
      timer = setInterval(() => {
        const now = performance.now();
        step(Math.min(0.1, (now - last) / 1000));
        last = now;
      }, 40);
    },
    stop() {
      clearInterval(timer);
      timer = null;
    },
  };
  for (const c of d.cams) for (const [slot, w] of [[0, 1], [1, 0.5], [2, 0.3]]) {
    const v = c.view;
    c.view = { cx: 0.5, cy: w === 1 ? 0.5 : 0.62, w: c.id === "cam-wide" ? Math.max(w, 0.6) : w };
    tools.presetSave({ camera: c.id, preset: slot + 1, name: ["Wide", "Stage", "Lectern"][slot] });
    c.view = v;
  }
  tally();
  return client;
}
