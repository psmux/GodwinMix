// Every call the camera window makes, in one place. The ptz and atem plugins
// are built alongside this panel and their tool names may still move; when
// they do, the TOOLS table below is the only thing to change. Each call goes
// through the core's public `tool.call` or a protocol method, the same way an
// agent or `gmx ctl` would make it.

export const TOOLS = {
  cameras: "ptz/camera_list",
  move: "ptz/move", // {camera, pan, tilt}: velocities from -1 to 1, stops by itself after 300 ms
  stop: "ptz/stop",
  zoom: "ptz/zoom", // {camera, speed}: -1 wide to 1 tight, 0 stops
  focus: "ptz/focus", // {camera, auto} or {camera, speed}
  exposure: "ptz/exposure", // {camera, mode, brightness}
  presets: "ptz/preset_list",
  presetSave: "ptz/preset_save", // answers with the preset and its thumbnail
  presetRecall: "ptz/preset_recall",
  track: "ptz/tracking", // {camera, on}
  framing: "ptz/framing", // {camera, style: close | waist | full | wide}
  observe: "track/observe", // {camera}: the people in the picture now
  follow: "track/follow", // {camera, person} or {camera, point: [x, y]}
  trackStop: "track/stop",
  trackState: "track/state",
  atemList: "atem/list",
  atemState: "atem/state",
  cut: "atem/cut",
  auto: "atem/auto",
  preview: "atem/preview",
  program: "atem/program",
  dsk: "atem/dsk",
  usk: "atem/usk",
  ftb: "atem/ftb",
  aux: "atem/aux",
  macro: "atem/macro",
  transition: "atem/transition",
  mode: "atem/mode",
};

/** A tool's answer: MCP's structured content, its JSON text, or the object itself. */
export function unwrap(r) {
  if (!r || typeof r !== "object") return r;
  if (r.structuredContent !== undefined) return r.structuredContent;
  const text = Array.isArray(r.content) && r.content.find((c) => c.type === "text");
  if (text) {
    try {
      return JSON.parse(text.text);
    } catch {
      return text.text;
    }
  }
  return r;
}

const list = (r, key) => (Array.isArray(r) ? r : (r && r[key]) || []);

export function cameraApi(client) {
  const tool = async (key, args) => unwrap(await client.call("tool.call", { name: TOOLS[key], arguments: args || {} }));
  const cam = (key) => (camera, more) => tool(key, { camera, ...more });
  const sw = (key) => (switcher, more) => tool(key, { switcher, ...more });
  return {
    client,
    tool,
    cameras: async () => list(await tool("cameras"), "cameras"),
    move: (camera, pan, tilt) => tool("move", { camera, pan, tilt }),
    stop: cam("stop"),
    zoom: (camera, speed) => tool("zoom", { camera, speed }),
    focus: cam("focus"),
    exposure: cam("exposure"),
    presets: async (camera) => list(await tool("presets", { camera }), "presets"),
    presetSave: (camera, slot, name) => tool("presetSave", { camera, slot, name }),
    presetRecall: (camera, slot) => tool("presetRecall", { camera, slot }),
    track: (camera, on) => tool("track", { camera, on }),
    framing: (camera, style) => tool("framing", { camera, style }),
    observe: cam("observe"),
    followPerson: (camera, person) => tool("follow", { camera, person }),
    followPoint: (camera, x, y) => tool("follow", { camera, point: [x, y] }),
    trackStop: cam("trackStop"),
    trackState: cam("trackState"),
    atems: async () => list(await tool("atemList").catch(() => []), "switchers"),
    atemState: sw("atemState"),
    atemCut: sw("cut"),
    atemAuto: sw("auto"),
    atemPreview: (switcher, input, me = 0) => tool("preview", { switcher, me, input }),
    atemProgram: (switcher, input, me = 0) => tool("program", { switcher, me, input }),
    atemDsk: (switcher, keyer, action) => tool("dsk", { switcher, keyer, action }),
    atemUsk: (switcher, keyer, on, me = 0) => tool("usk", { switcher, me, keyer, on }),
    atemFtb: sw("ftb"),
    atemAux: (switcher, aux, input) => tool("aux", { switcher, aux, input }),
    atemMacro: (switcher, index) => tool("macro", { switcher, index, action: "run" }),
    atemTransition: (switcher, more) => tool("transition", { switcher, me: 0, ...more }),
    atemMode: (switcher, mode) => tool("mode", { switcher, mode }),
    // GodwinMix's own programme, by protocol method.
    take: (source, transition) => client.call("program.take", transition ? { source, transition } : { source }),
    black: () => client.call("program.take", { source: "" }),
    key: (scene, on) => client.call("program.key.set", { scene, on }),
  };
}
