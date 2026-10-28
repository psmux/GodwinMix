// The picture settings past size and bitrate, with no DOM: the frame rate,
// the shape a source is shown at, how a deinterlace makes its lines, a
// larger source kept at its own size, the rungs of a profile's HLS ladder,
// and a channel's own settings over its profile's. Shapes are VideoWant's
// (crates/godwinmix-protocol/src/rendition) and TranscodeProfile's
// (headend/profile.rs). Each goes into a want only when it says something
// the default does not, so a form left alone sends what it always sent.

export const FPS = [["", "As it comes"], ["24/1", "24"], ["25/1", "25"], ["30000/1001", "29.97"], ["30/1", "30"], ["50/1", "50"], ["60000/1001", "59.94"], ["60/1", "60"]];
export const ASPECT = [["auto", "As the stream says"], ["16:9", "16:9, whatever the stream says"], ["4:3", "4:3, whatever the stream says"]];
export const METHOD = [["yadif", "YADIF, the sharpest (recommended)"], ["greedyh", "Greedy H, a little cheaper and softer"], ["linear", "Linear, the cheapest and softest"]];
export const MATCH = [["", "Off: every source at the size above"], ["1080", "Up to 1080p"], ["1440", "Up to 1440p"], ["2160", "Up to 2160p (4K)"]];

const whole = (v) => (String(v ?? "").trim() !== "" && Number(v) > 0 ? Math.round(Number(v)) : null);

export const fpsKey = (fps) => (fps && fps.num ? `${fps.num}/${fps.den || 1}` : "");

export function fpsFrom(key) {
  const m = /^(\d+)\/(\d+)$/.exec(String(key || ""));
  return m && Number(m[1]) > 0 && Number(m[2]) > 0 ? { num: Number(m[1]), den: Number(m[2]) } : null;
}

/** A choice's rows with the current value in them, for a value the list lacks. */
export function withValue(rows, value, words) {
  return !value || rows.some(([v]) => v === value) ? rows : [...rows, [value, words || value]];
}

/** The extra fields from a VideoWant, flat, as the forms hold them. */
export function extraFields(v) {
  const w = v || {};
  const m = w.match_source || null;
  return {
    fps: fpsKey(w.fps),
    aspect: w.aspect || "auto",
    deinterlace_method: w.deinterlace_method || "yadif",
    match_height: m && m.max_height ? String(m.max_height) : "",
    match_kbps: m && m.bitrate_kbps ? String(m.bitrate_kbps) : "",
  };
}

/** The extra fields written into a VideoWant, in place. */
export function extrasInto(v, f) {
  const fps = fpsFrom(f.fps);
  if (fps) v.fps = fps;
  if (f.aspect === "16:9" || f.aspect === "4:3") v.aspect = f.aspect;
  // The method means something only where a deinterlace runs.
  if (v.deinterlace && v.deinterlace !== "off" && f.deinterlace_method && f.deinterlace_method !== "yadif") v.deinterlace_method = f.deinterlace_method;
  const top = whole(f.match_height);
  if (top && v.height) {
    v.match_source = { max_height: top };
    if (whole(f.match_kbps)) v.match_source.bitrate_kbps = whole(f.match_kbps);
  }
  return v;
}

// The ladder: each rung a size and a bitrate, and a frame rate and preset
// of its own where it needs them. What a rung leaves out is the main
// picture's, so the rest of what it came with is kept as it was.

const RUNG_KEYS = ["width", "height", "bitrate_kbps", "fps", "preset"];
const STEPS = [[1920, 1080, 4500], [1280, 720, 1500], [1024, 576, 1100], [854, 480, 800], [640, 360, 500], [426, 240, 300]];

/** A rung's fields, flat. */
export const rungFields = (r) => ({
  size: r && r.width && r.height ? `${r.width}x${r.height}` : "1280x720",
  bitrate_kbps: r && r.bitrate_kbps ? String(r.bitrate_kbps) : "",
  fps: fpsKey(r && r.fps),
  preset: (r && r.preset) || "",
});

/** A rung from its fields, over `keep`, the rung as it came. */
export function rungFrom(f, keep = {}) {
  const r = Object.fromEntries(Object.entries(keep).filter(([k]) => !RUNG_KEYS.includes(k) && k !== "match_source"));
  const [w, h] = String(f.size || "").split("x").map(Number);
  if (w > 0 && h > 0) Object.assign(r, { width: w, height: h });
  if (whole(f.bitrate_kbps)) r.bitrate_kbps = whole(f.bitrate_kbps);
  const fps = fpsFrom(f.fps);
  if (fps) r.fps = fps;
  if (String(f.preset || "").trim()) r.preset = String(f.preset).trim();
  return r;
}

/** The next rung down: the step below the smallest picture there is. */
export function nextRung(video, rungs) {
  const heights = [video && video.height, ...(rungs || []).map((r) => r.height)].filter(Boolean);
  const low = heights.length ? Math.min(...heights) : 1080;
  const [width, height, bitrate_kbps] = STEPS.find(([, h]) => h < low) || STEPS.at(-1);
  return { width, height, bitrate_kbps };
}

/** What is wrong with a ladder, or "". */
export function ladderProblem(video, rungs) {
  if (!rungs.length) return "";
  if (!video) return "A ladder needs the picture converted: pick a video codec first.";
  const heights = [video.height, ...rungs.map((r) => r.height)].filter(Boolean);
  return new Set(heights).size === heights.length ? "" : "Two sizes in the ladder are the same. Change or remove one.";
}

// A channel's own picture settings over its profile's. "" in a field is
// the profile's own; an empty result means the profile as it is.

export const OVERRIDE_KEYS = ["aspect", "deinterlace", "deinterlace_method", "fit"];

export const overrideFields = (o) => Object.fromEntries(OVERRIDE_KEYS.map((k) => [k, (o && o[k]) || ""]));

export function overridesFrom(f) {
  const o = {};
  for (const k of OVERRIDE_KEYS) if (f[k]) o[k] = f[k];
  if (o.deinterlace === "off") delete o.deinterlace_method;
  return o;
}

/** "shown as 4:3, deinterlace on with greedyh, crop", or "". */
export function overrideWords(o) {
  if (!o) return "";
  const bits = [];
  if (o.aspect) bits.push(o.aspect === "auto" ? "shape as the stream says" : `shown as ${o.aspect}`);
  if (o.deinterlace) bits.push(`deinterlace ${o.deinterlace}`);
  if (o.deinterlace_method) bits.push(`${o.deinterlace_method} lines`);
  if (o.fit) bits.push(o.fit);
  return bits.join(", ");
}
