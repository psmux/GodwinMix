// An HLS output's settings past the one click of low latency: which
// segments it makes, who may play it, the segment length, the window, the
// LL-HLS part and a viewer key of its own. The schema kit draws them as its
// Advanced group, closed, the same as every other Advanced section here.
//
// The names are `params` on `output.add` and on `show.output.add` and
// `show.output.set` (docs/reference/hls-output.md). `show.output.set`
// replaces every param, so `paramsFrom` always answers the whole set.

import { stylesheet } from "./shared.js";

const ADV = "Advanced";

export const OPEN_HINT = "Open is for an output behind a gateway that checks its own signature. It answers a player on this machine or the local network with no key, and a player anywhere else still needs the key.";

const SEGMENTS = [["cmaf", "CMAF (fragmented MP4), the default"], ["both", "CMAF and MPEG-TS, cut on the same keyframes"], ["ts", "MPEG-TS only"]];
const ACCESS = [["keyed", "With the link's key, the default"], ["open", "Open, behind a gateway"]];
const choice = (rows) => rows.map(([v, t]) => ({ const: v, title: t }));

/** Segment length and parts for the one click: normal or low latency. */
export function latencyDefaults(low) {
  return low ? { segment_ms: 1000, part_ms: 200 } : { segment_ms: 2000, part_ms: 0 };
}

/**
 * The Advanced fields. `direct` is an output of a show without compositing,
 * the only kind that makes MPEG-TS so far.
 */
export function hlsSchema({ direct } = {}) {
  const props = {};
  if (direct) props.segments = { type: "string", title: "Segments", oneOf: choice(SEGMENTS), default: "cmaf", "x-gmx-group": ADV, description: "MPEG-TS is for set top boxes and players that take nothing else." };
  props.access = { type: "string", title: "Who may play it", oneOf: choice(ACCESS), default: "keyed", "x-gmx-group": ADV, description: OPEN_HINT };
  props.open_public = { type: "boolean", title: "Open to every address", default: false, "x-gmx-group": ADV, description: "Answer players outside this machine and the local network too, with no key: the whole internet, if the port is reachable." };
  props.segment_ms = { type: "integer", title: "Segment length", minimum: 500, maximum: 10000, "x-gmx-unit": "ms", "x-gmx-group": ADV, description: "Cut at the first keyframe at or after it. Left empty, 2000, or 1000 with low latency." };
  props.window = { type: "integer", title: "Playlist window", minimum: 1, maximum: 600, default: 30, "x-gmx-unit": "s", "x-gmx-group": ADV, description: "Seconds of the past each playlist lists and keeps, at least three segments." };
  props.part_ms = { type: "integer", title: "Low latency part", minimum: 0, maximum: 5000, "x-gmx-unit": "ms", "x-gmx-group": ADV, description: "0 for none. Left empty, 200 with low latency." };
  props.viewer_key = { type: "string", title: "Viewer key", "x-gmx-group": ADV, description: "16 letters, digits, - or _ or more. Left empty, the key this machine makes from the output's name, which a restart keeps." };
  return {
    type: "object",
    properties: props,
    allOf: [{ if: { properties: { access: { const: "open" } } }, then: { properties: { open_public: {} } } }],
  };
}

/** The form's starting answers from an output's current params. */
export function valuesFromParams(p) {
  const out = {};
  for (const k of ["segments", "access", "open_public", "segment_ms", "window", "part_ms", "viewer_key"]) if (p && p[k] !== undefined && p[k] !== null) out[k] = p[k];
  return out;
}

/** True when the params ask for anything the one click does not. */
export function usesAdvanced(p) {
  if (!p) return false;
  return (p.segments && p.segments !== "cmaf") || p.access === "open" || !!p.viewer_key || (p.window && p.window !== 30) || (p.segment_ms && ![1000, 2000].includes(p.segment_ms));
}

/** The whole params to send, from the low latency click and the Advanced answers. */
export function paramsFrom(v, low) {
  const base = latencyDefaults(low);
  const params = { segment_ms: Number(v.segment_ms) > 0 ? Number(v.segment_ms) : base.segment_ms, window: Number(v.window) > 0 ? Number(v.window) : 30 };
  const part = v.part_ms === undefined || v.part_ms === "" ? base.part_ms : Number(v.part_ms);
  if (part > 0) params.part_ms = part;
  if (v.segments && v.segments !== "cmaf") params.segments = v.segments;
  if (v.access === "open") {
    params.access = "open";
    if (v.open_public) params.open_public = true;
  }
  if (String(v.viewer_key || "").trim()) params.viewer_key = String(v.viewer_key).trim();
  return params;
}

/** The Advanced form, built by the schema kit on first use. */
export async function hlsAdvanced(opts = {}) {
  stylesheet();
  const { SchemaInspector } = await import("../../kits/schema/index.js");
  const form = new SchemaInspector({ schema: hlsSchema(opts), value: valuesFromParams(opts.params) });
  form.el.classList.add("rnd-hlsadv");
  const box = form.el.querySelector("details");
  if (box && usesAdvanced(opts.params)) box.open = true;
  return { el: form.el, read: (low) => paramsFrom(form.read(), low) };
}
