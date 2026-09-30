// The Custom card's form: one rendition, field by field.
//
// The schema is RenditionRequest's (crates/godwinmix-protocol/src/rendition)
// with the pieces a person picks together put together: a size is one choice
// rather than a width and a height, and the sound is one choice of codec and
// rate. `requestFrom` turns the answers back into the wire's shape, and the
// form itself is the schema kit's, so it looks like every other form here.

const SIZES = [
  ["3840x2160", "3840×2160 (4K)"],
  ["1920x1080", "1920×1080 (1080p)"],
  ["1280x720", "1280×720 (720p)"],
  ["854x480", "854×480 (480p)"],
  ["640x360", "640×360 (360p)"],
];
const CODECS = [
  ["h264", "H.264, plays everywhere"],
  ["h265", "HEVC, half the bits, newer players"],
  ["av1", "AV1, smallest, slowest to make"],
  ["vp9", "VP9, for WebRTC"],
];
const SOUND = [
  ["aac-160", "AAC 160 kb/s"],
  ["aac-128", "AAC 128 kb/s"],
  ["aac-96", "AAC 96 kb/s"],
  ["opus-128", "Opus 128 kb/s"],
  ["copy", "Same as the source"],
  ["none", "No sound"],
];

const choice = (rows, keep) => rows.filter(([v]) => !keep || keep.has(v)).map(([v, t]) => ({ const: v, title: t }));

/**
 * The form's schema. `codecs` narrows the video codecs to the ones this
 * machine can encode, when the caller knows them; H.264 is always there.
 */
export function customSchema(codecs) {
  const keep = codecs && codecs.size ? new Set(["h264", ...codecs]) : null;
  return {
    type: "object",
    properties: {
      codec: { type: "string", title: "Video codec", oneOf: choice(CODECS, keep), default: "h264" },
      size: { type: "string", title: "Picture size", oneOf: choice(SIZES), default: "1280x720" },
      fps: { type: "number", title: "Frame rate", oneOf: [60, 50, 30, 25].map((n) => ({ const: n, title: `${n} fps` })), default: 30 },
      bitrate_kbps: { type: "integer", title: "Video bitrate", minimum: 200, maximum: 50000, default: 3000, "x-gmx-unit": "kb/s" },
      keyframe_s: {
        type: "number", title: "Keyframe every", minimum: 0.5, maximum: 10, default: 2, "x-gmx-unit": "s",
        description: "Two seconds is what the platforms ask for.",
      },
      sound: { type: "string", title: "Sound", oneOf: choice(SOUND), default: "aac-160" },
    },
  };
}

/** The form's answers as a RenditionRequest. */
export function requestFrom(v, id) {
  const [w, h] = String(v.size || "1280x720").split("x").map(Number);
  const req = {
    id,
    container: "flv",
    video: {
      codec: v.codec || "h264",
      width: w,
      height: h,
      fps: { num: Number(v.fps) || 30, den: 1 },
      bitrate_kbps: Math.round(Number(v.bitrate_kbps) || 3000),
      keyframe_ms: Math.round((Number(v.keyframe_s) || 2) * 1000),
    },
  };
  const sound = v.sound || "aac-160";
  if (sound === "none") req.no_audio = true;
  else if (sound !== "copy") {
    const [codec, kbps] = sound.split("-");
    req.audio = { codec, bitrate_kbps: Number(kbps) };
  }
  return req;
}

/** The answers a request fills the form with, so an edit opens on what is set. */
export function valuesFrom(req) {
  const v = (req && req.video) || {};
  const a = (req && req.audio) || {};
  const out = {};
  if (v.codec) out.codec = v.codec;
  if (v.width && v.height) out.size = `${v.width}x${v.height}`;
  if (v.fps) out.fps = Math.round(v.fps.num / (v.fps.den || 1));
  if (v.bitrate_kbps) out.bitrate_kbps = v.bitrate_kbps;
  if (v.keyframe_ms) out.keyframe_s = v.keyframe_ms / 1000;
  if (req && req.no_audio) out.sound = "none";
  else if (a.codec) out.sound = `${a.codec}-${a.bitrate_kbps || 128}`;
  return out;
}

/** The form, built by the schema kit on first use. */
export async function customForm(opts = {}) {
  const { SchemaInspector } = await import("../../kits/schema/index.js");
  const form = new SchemaInspector({ schema: customSchema(opts.codecs), value: valuesFrom(opts.request), onChange: opts.onChange });
  form.el.classList.add("rnd-customform");
  return { el: form.el, request: (id) => requestFrom(form.read(), id) };
}
