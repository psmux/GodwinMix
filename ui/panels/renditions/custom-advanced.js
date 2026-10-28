// The Custom form's Advanced group: how a picture of another shape is
// fitted, the shape the source is shown at, deinterlacing and its method,
// a keyframe interval in frames, the encoder's own
// preset and B frames, and which sound tracks are converted at what rate.
//
// These are VideoWant's and AudioWant's newer fields
// (crates/godwinmix-protocol/src/rendition). Each one goes into the request
// only when it says something the default does not, so a form left alone
// sends what it always sent and the planner prices it the same.

const ADV = "Advanced";
const pick = (rows) => rows.map(([v, t]) => ({ const: v, title: t }));

const FIT = [["pad", "Pad with bars"], ["crop", "Crop to fill"], ["stretch", "Stretch to fill"]];
const DEINTERLACE = [["off", "Off, scale as it comes"], ["on", "On, every picture"], ["auto", "Only pictures marked interlaced"]];
const ASPECT = [["auto", "As the stream says"], ["16:9", "16:9, whatever the stream says"], ["4:3", "4:3, whatever the stream says"]];
const METHOD = [["yadif", "YADIF, the sharpest"], ["greedyh", "Greedy H, a little cheaper and softer"], ["linear", "Linear, the cheapest and softest"]];
const TRACKS = [["first", "The first track only"], ["all", "Every track, each on its own"]];

/** The Advanced fields, to sit beside the Custom form's own. */
export function advancedProps() {
  return {
    fit: { type: "string", title: "Another shape", oneOf: pick(FIT), default: "pad", "x-gmx-group": ADV, description: "How a picture of another shape goes into the size. Pad keeps all of it." },
    pad_color: { type: "string", title: "Bar colour", default: "#000000", "x-gmx-group": ADV, examples: ["#000000"], description: "#rrggbb. Some conversions make black bars only, and the plan says so." },
    aspect: { type: "string", title: "Source shape", oneOf: pick(ASPECT), default: "auto", "x-gmx-group": ADV, description: "For a source that flags its shape wrong. Pad and crop fit by this shape." },
    deinterlace: { type: "string", title: "Deinterlace", oneOf: pick(DEINTERLACE), default: "off", "x-gmx-group": ADV, description: "Deinterlacing runs on the CPU between a GPU decode and the encode." },
    deinterlace_method: { type: "string", title: "Deinterlace method", oneOf: pick(METHOD), default: "yadif", "x-gmx-group": ADV, description: "On the CPU whatever the method: there is no GPU deinterlace. The scale and the encode stay on the GPU." },
    keyframe_frames: { type: "integer", title: "Keyframe every, in frames", minimum: 1, maximum: 1000, "x-gmx-group": ADV, description: "Of the source's own rate. Taken over the seconds above when set." },
    preset: { type: "string", title: "Encoder preset", "x-gmx-group": ADV, examples: ["p1"], description: "In the encoder's own words: p1 to p7 on NVENC, veryfast on x264. Left empty, the catalogue's." },
    bframes: { type: "integer", title: "B frames", minimum: 0, maximum: 4, "x-gmx-group": ADV, description: "Between references. Left empty, none, which is what live wants." },
    tracks: { type: "string", title: "Sound tracks", oneOf: pick(TRACKS), default: "first", "x-gmx-group": ADV, description: "Every track keeps its own channels and language." },
    audio_kbps: { type: "integer", title: "Sound bitrate", minimum: 32, maximum: 512, "x-gmx-unit": "kb/s", "x-gmx-group": ADV, description: "Per track. Left empty, the rate of the Sound choice." },
  };
}

/** Bar colour is asked only for a pad, the method only where a deinterlace runs. */
export const advancedRules = [
  { if: { properties: { fit: { const: "pad" } } }, then: { properties: { pad_color: {} } } },
  { if: { properties: { deinterlace: { enum: ["on", "auto"] } } }, then: { properties: { deinterlace_method: {} } } },
];

const COLOR = /^#[0-9a-fA-F]{6}$/;

/** Put the Advanced answers into a request built from the rest. */
export function advancedInto(req, v) {
  const video = req.video;
  if (video) {
    if (v.fit && v.fit !== "pad") video.fit = v.fit;
    if ((v.fit || "pad") === "pad" && COLOR.test(v.pad_color || "") && v.pad_color.toLowerCase() !== "#000000") video.pad_color = v.pad_color.toLowerCase();
    if (v.aspect === "16:9" || v.aspect === "4:3") video.aspect = v.aspect;
    if (v.deinterlace && v.deinterlace !== "off") video.deinterlace = v.deinterlace;
    if (video.deinterlace && v.deinterlace_method && v.deinterlace_method !== "yadif") video.deinterlace_method = v.deinterlace_method;
    if (Number(v.keyframe_frames) > 0) video.keyframe_frames = Math.round(Number(v.keyframe_frames));
    if (String(v.preset || "").trim()) video.preset = String(v.preset).trim();
    if (v.bframes !== undefined && v.bframes !== "" && Number(v.bframes) >= 0) video.bframes = Math.round(Number(v.bframes));
  }
  if (req.audio) {
    if (v.tracks === "all") req.audio.tracks = "all";
    if (Number(v.audio_kbps) > 0) req.audio.bitrate_kbps = Math.round(Number(v.audio_kbps));
  }
  return req;
}

/** The Advanced answers a request fills the form with. */
export function advancedFrom(req) {
  const v = (req && req.video) || {};
  const a = (req && req.audio) || {};
  const out = {};
  for (const k of ["fit", "pad_color", "aspect", "deinterlace", "deinterlace_method", "keyframe_frames", "preset", "bframes"]) if (v[k] !== undefined && v[k] !== null) out[k] = v[k];
  if (a.tracks) out.tracks = a.tracks;
  return out;
}

/** True when the request asks for anything the Advanced group sets. */
export function usesAdvanced(req) {
  const v = (req && req.video) || {};
  return ["keyframe_frames", "preset", "bframes", "deinterlace_method"].some((k) => v[k] !== undefined) || (v.aspect && v.aspect !== "auto") || (v.fit && v.fit !== "pad") || (v.deinterlace && v.deinterlace !== "off") || ((req && req.audio) || {}).tracks === "all";
}
