// Actions, feedbacks, variables and presets, as data.
//
// Companion asks a module for four tables and then calls back into it. The
// tables are plain objects and the callbacks are plain functions of a [`Link`]
// and the options an operator filled in, so both halves are testable with no
// Companion in the process. `index.ts` is the only file that imports
// `@companion-module/base`, and all it does is hand these tables over.

import { clock, type Link, type View } from "./link.ts";

/** What Companion hands a callback: the options the operator filled in. */
export type Options = Record<string, unknown>;

function text(options: Options, key: string, fallback = ""): string {
  const value = options[key];
  return typeof value === "string" ? value.trim() : fallback;
}

function number(options: Options, key: string, fallback: number): number {
  const value = options[key];
  const parsed = typeof value === "number" ? value : Number(value);
  return Number.isFinite(parsed) ? parsed : fallback;
}

// ---------------------------------------------------------------------------
// Actions
// ---------------------------------------------------------------------------

/** Every action, by id, with the call each one makes. */
export const ACTIONS = {
  take: {
    name: "Take a source to programme",
    options: [
      { type: "textinput", id: "source", label: "Source id", default: "cam1", useVariables: true },
    ],
    run: (link: Link, options: Options) => link.take(text(options, "source") || null),
  },
  take_slate: {
    name: "Cut to the slate",
    options: [],
    run: (link: Link) => link.take(null),
  },
  take_scene: {
    name: "Take a scene",
    options: [
      { type: "textinput", id: "scene", label: "Scene name", default: "", useVariables: true },
    ],
    run: (link: Link, options: Options) => link.takeScene(text(options, "scene")),
  },
  revert: {
    name: "Back to the shot before",
    options: [],
    run: (link: Link) => link.revert(),
  },
  add_source: {
    name: "Add a source",
    options: [
      { type: "textinput", id: "uri", label: "Address", default: "", useVariables: true },
      { type: "textinput", id: "id", label: "Id (optional)", default: "", useVariables: true },
      { type: "textinput", id: "name", label: "Name (optional)", default: "", useVariables: true },
    ],
    run: (link: Link, options: Options) =>
      link.addSource(text(options, "uri"), text(options, "id") || undefined, text(options, "name") || undefined),
  },
  start_output: {
    name: "Start an output",
    options: [
      { type: "textinput", id: "uri", label: "Destination", default: "", useVariables: true },
      { type: "textinput", id: "id", label: "Id (optional)", default: "", useVariables: true },
    ],
    run: (link: Link, options: Options) =>
      link.startOutput(text(options, "uri"), text(options, "id") || undefined),
  },
  stop_output: {
    name: "Stop an output",
    options: [{ type: "textinput", id: "id", label: "Output id", default: "", useVariables: true }],
    run: (link: Link, options: Options) => link.stopOutput(text(options, "id")),
  },
  reconnect_output: {
    name: "Reconnect an output",
    options: [{ type: "textinput", id: "id", label: "Output id", default: "", useVariables: true }],
    run: (link: Link, options: Options) => link.reconnectOutput(text(options, "id")),
  },
  set_gain: {
    name: "Set a source's fader",
    options: [
      { type: "textinput", id: "id", label: "Source id", default: "cam1", useVariables: true },
      { type: "number", id: "gain", label: "Gain (1.0 is unity)", default: 1, min: 0, max: 10, step: 0.05 },
    ],
    run: (link: Link, options: Options) => link.setAudio(text(options, "id"), number(options, "gain", 1)),
  },
  set_mute: {
    name: "Mute or unmute a source",
    options: [
      { type: "textinput", id: "id", label: "Source id", default: "cam1", useVariables: true },
      { type: "checkbox", id: "muted", label: "Muted", default: true },
    ],
    run: (link: Link, options: Options) =>
      link.setAudio(text(options, "id"), undefined, options.muted !== false),
  },
} as const;

export type ActionId = keyof typeof ACTIONS;

// ---------------------------------------------------------------------------
// Feedbacks
// ---------------------------------------------------------------------------

/** Companion's colours, as numbers, so this file needs no import to make one. */
export const RGB = {
  black: 0x000000,
  white: 0xffffff,
  red: 0xcc0000,
  green: 0x00aa00,
  amber: 0xcc8800,
  dark: 0x1a1a1a,
} as const;

/**
 * Every feedback, by id. `check` answers whether the feedback is on for this
 * view and these options, and that is the whole of the logic: Companion turns
 * `true` into the style the operator chose.
 */
export const FEEDBACKS = {
  tally: {
    type: "boolean" as const,
    name: "Source is on programme",
    description:
      "Red while this source is on air. Follows event/tally, which the core derives, so it is right even when somebody else took the source.",
    defaultStyle: { bgcolor: RGB.red, color: RGB.white },
    options: [
      { type: "textinput", id: "source", label: "Source id", default: "cam1", useVariables: true },
    ],
    check: (view: View, options: Options) => view.tally[text(options, "source")] === "program",
  },
  tally_preview: {
    type: "boolean" as const,
    name: "Source is on preview",
    description: "Green while this source is in the armed scene.",
    defaultStyle: { bgcolor: RGB.green, color: RGB.white },
    options: [
      { type: "textinput", id: "source", label: "Source id", default: "cam1", useVariables: true },
    ],
    check: (view: View, options: Options) => view.tally[text(options, "source")] === "preview",
  },
  output_state: {
    type: "boolean" as const,
    name: "Output is in a state",
    description: "On while this destination is connecting, live, reconnecting or failed.",
    defaultStyle: { bgcolor: RGB.green, color: RGB.white },
    options: [
      { type: "textinput", id: "id", label: "Output id", default: "", useVariables: true },
      {
        type: "dropdown",
        id: "state",
        label: "State",
        default: "live",
        choices: [
          { id: "live", label: "live" },
          { id: "connecting", label: "connecting" },
          { id: "reconnecting", label: "reconnecting" },
          { id: "failed", label: "failed" },
        ],
      },
    ],
    check: (view: View, options: Options) =>
      view.outputs[text(options, "id")] === text(options, "state", "live"),
  },
  connected: {
    type: "boolean" as const,
    name: "Connected to the mixer",
    description: "On while this Companion instance has a connection to the mixer.",
    defaultStyle: { bgcolor: RGB.dark, color: RGB.white },
    options: [],
    check: (view: View) => view.connected,
  },
} as const;

export type FeedbackId = keyof typeof FEEDBACKS;

// ---------------------------------------------------------------------------
// Variables
// ---------------------------------------------------------------------------

export const VARIABLES = [
  { variableId: "program", name: "Source on programme" },
  { variableId: "program_name", name: "Name of the source on programme" },
  { variableId: "uptime", name: "How long the mixer has been up" },
  { variableId: "uptime_secs", name: "How long the mixer has been up, in seconds" },
  { variableId: "running_time", name: "Programme running time" },
  { variableId: "source_count", name: "How many sources there are" },
  { variableId: "live_source_count", name: "How many sources are live" },
  { variableId: "output_count", name: "How many destinations there are" },
  { variableId: "connected", name: "Connected to the mixer" },
] as const;

/** The values, read off one view. */
export function variableValues(view: View): Record<string, string | number> {
  const onAir = view.sources.find((source) => source.id === view.program);
  return {
    program: view.program ?? "",
    program_name: onAir?.name ?? (view.program ?? ""),
    uptime: clock(view.uptimeSecs),
    uptime_secs: view.uptimeSecs,
    running_time: clock(Math.floor(view.runningTimeMs / 1000)),
    source_count: view.sources.length,
    live_source_count: view.sources.filter((source) => source.state === "live").length,
    output_count: Object.keys(view.outputs).length,
    connected: view.connected ? "yes" : "no",
  };
}

// ---------------------------------------------------------------------------
// Presets
// ---------------------------------------------------------------------------

/**
 * The buttons an operator gets for free after adding the connection.
 *
 * Four camera take buttons that light red on air, a slate, a revert and a
 * connection lamp. A preset is a button ready to drag onto a page, which is
 * the difference between a module somebody tries and a module somebody keeps.
 */
export function presets(): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (let n = 1; n <= 4; n += 1) {
    const id = `cam${n}`;
    out[`take_${id}`] = {
      type: "button",
      category: "Take",
      name: `Take ${id}`,
      style: { text: `CAM ${n}`, size: "18", color: RGB.white, bgcolor: RGB.dark },
      steps: [{ down: [{ actionId: "take", options: { source: id } }], up: [] }],
      feedbacks: [
        { feedbackId: "tally", options: { source: id }, style: { bgcolor: RGB.red, color: RGB.white } },
        {
          feedbackId: "tally_preview",
          options: { source: id },
          style: { bgcolor: RGB.green, color: RGB.white },
        },
      ],
    };
  }
  out.slate = {
    type: "button",
    category: "Take",
    name: "Cut to the slate",
    style: { text: "SLATE", size: "14", color: RGB.white, bgcolor: RGB.black },
    steps: [{ down: [{ actionId: "take_slate", options: {} }], up: [] }],
    feedbacks: [],
  };
  out.revert = {
    type: "button",
    category: "Take",
    name: "Back to the shot before",
    style: { text: "BACK", size: "14", color: RGB.white, bgcolor: RGB.dark },
    steps: [{ down: [{ actionId: "revert", options: {} }], up: [] }],
    feedbacks: [],
  };
  out.connected = {
    type: "button",
    category: "Status",
    name: "Connected to the mixer",
    style: { text: "MIXER\\n$(gmx:program)", size: "14", color: RGB.white, bgcolor: RGB.black },
    steps: [{ down: [], up: [] }],
    feedbacks: [
      { feedbackId: "connected", options: {}, style: { bgcolor: RGB.green, color: RGB.white } },
    ],
  };
  return out;
}

/** The fields an operator fills in when adding the connection. */
export function configFields(): unknown[] {
  return [
    {
      type: "static-text",
      id: "info",
      width: 12,
      label: "GodwinMix",
      value:
        "The address of the mixer's control server, as you would type it into a browser, and a token with the operate scope.",
    },
    { type: "textinput", id: "base", label: "Address", width: 8, default: "http://127.0.0.1:8080" },
    { type: "textinput", id: "token", label: "Token", width: 4, default: "" },
  ];
}
