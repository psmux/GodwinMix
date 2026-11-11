// Every call camera properties, the Preview overlay and the switcher drawer
// make, in one place. The camera tools are the
// ptz plugin's (plugins/ptz/gmx-plugin.toml), reached as `ptz/<tool>` through
// the core's public `tool.call`, the way an agent or `gmx ctl` reaches them.
// The switcher tools are the atem plugin's (plugins/atem/gmx-plugin.toml),
// reached as `atem/<tool>`. When a name moves, the TOOLS table below is the
// only thing to change; when an answer's shape moves, shapes.js is.

import * as shape from "./shapes.js";

export const TOOLS = {
  cameras: "ptz/camera_list",
  add: "ptz/camera_add", // {id, protocol, address, source}
  move: "ptz/move", // {camera, pan, tilt, zoom}: -1 to 1, held 300 ms
  stop: "ptz/stop", // stops the camera and turns its tracking off
  focus: "ptz/focus", // {camera, mode: auto | manual | one_push | near | far | stop, speed}
  exposure: "ptz/exposure", // {camera, mode: auto | manual | shutter | iris, iris, shutter, gain}
  position: "ptz/position",
  presets: "ptz/preset_list", // {camera, pictures: true}
  presetSave: "ptz/preset_save", // {camera, preset, name}
  presetRecall: "ptz/preset_recall", // {camera, preset}
  follow: "ptz/track_follow", // {camera, framing, target | point: {x, y}}: tracking on
  trackStop: "ptz/track_stop",
  framing: "ptz/track_framing", // {camera, framing}
  trackState: "ptz/track_state",
  observe: "ptz/track_observe", // {camera, wait_ms}
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
  connect: "atem/connect", // {id, address, name, mode, map}
  disconnect: "atem/disconnect",
  map: "atem/map", // {switcher, source, input | null}
};

/**
 * A tool's answer. The core hands back what the plugin answered, in MCP's
 * shape with snake case keys: {content, structured_content, is_error}. The
 * facts are in structured_content; content is a sentence for a person,
 * which is what a refusal flagged with is_error says.
 */
export function unwrap(r) {
  if (!r || typeof r !== "object") return r;
  const text = Array.isArray(r.content) ? r.content.filter((c) => c && c.type === "text").map((c) => c.text).join(" ") : "";
  if (r.is_error || r.isError) throw new Error(text || "the tool refused");
  if (r.structured_content !== undefined && r.structured_content !== null) return r.structured_content;
  if (r.structuredContent !== undefined) return r.structuredContent;
  if (text) {
    try {
      return JSON.parse(text);
    } catch {
      return { text };
    }
  }
  return r;
}

const list = (r, key) => (Array.isArray(r) ? r : (r && r[key]) || []);

export function cameraApi(client) {
  const tool = async (key, args) => unwrap(await client.call("tool.call", { name: TOOLS[key], arguments: args || {} }));
  const sw = (key) => (switcher, more) => tool(key, { switcher, ...more });
  return {
    client,
    tool,
    cameras: async () => list(await tool("cameras"), "cameras").map(shape.camera),
    add: (camera) => tool("add", camera),
    move: (camera, pan, tilt, zoom = 0) => tool("move", { camera, pan, tilt, zoom }),
    /** Let go: zero speeds, which leaves tracking as it was. The stop tool would turn it off. */
    still: (camera) => tool("move", { camera, pan: 0, tilt: 0, zoom: 0 }),
    stop: (camera) => tool("stop", { camera }),
    focus: (camera, mode, speed) => tool("focus", speed === undefined ? { camera, mode } : { camera, mode, speed }),
    exposure: (camera, set) => tool("exposure", { camera, ...set }),
    presets: async (camera) => list(await tool("presets", { camera, pictures: true }), "presets").map(shape.preset),
    presetSave: (camera, preset, name) => tool("presetSave", { camera, preset, name }),
    presetRecall: (camera, preset) => tool("presetRecall", { camera, preset }),
    /** Tracking on (following whoever is central, or `framing`'s next follow) or off. */
    track: (camera, on, framing) => (on ? tool("follow", framing ? { camera, framing, target: "central" } : { camera, target: "central" }) : tool("trackStop", { camera })),
    framing: (camera, framing) => tool("framing", { camera, framing }),
    observe: async (camera) => shape.people(await tool("observe", { camera, wait_ms: 0 })),
    followPerson: (camera, target) => tool("follow", { camera, target }),
    followPoint: (camera, x, y) => tool("follow", { camera, point: { x, y } }),
    trackStop: (camera) => tool("trackStop", { camera }),
    trackState: async (camera) => shape.tracking(await tool("trackState", { camera })),
    atems: async () => list(await tool("atemList").catch(() => []), "switchers"),
    atemState: async (switcher) => shape.atem(await tool("atemState", { switcher })),
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
    atemConnect: (set) => tool("connect", set),
    atemDisconnect: (switcher) => tool("disconnect", { switcher }),
    atemMap: (switcher, source, input) => tool("map", { switcher, source, input }),
  };
}
