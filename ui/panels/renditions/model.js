// What a format is, in words and in a badge, and which one to start on.
//
// Pure: no DOM and no client, so the tests can ask it anything. The presets
// come from `rendition.presets` (dev/plans/wave2-contract.md); the source's
// shape comes from wherever the caller knows it, the programme's settings or
// a channel's live stream.

/** The card that is not a preset: send the source's own bytes. */
export const COPY = "copy";

/**
 * What each platform is offered first, best first. The first one this
 * machine can make is the suggestion; a platform with none gets copy.
 */
const SUGGEST = {
  youtube: ["youtube-1080p30", "youtube-720p30"],
  facebook: ["facebook-720p30"],
  twitch: ["twitch-1080p60", "twitch-720p30"],
  kick: ["twitch-1080p60", "twitch-720p30"],
  x: ["youtube-720p30"],
  linkedin: ["youtube-720p30"],
};

/** The presets one format step offers: single renditions, never a ladder or copy. */
export function singles(presets) {
  return (presets || []).filter((p) => p.id !== COPY && !(p.ladder && p.ladder.length));
}

/** The ladders the HLS picker offers. */
export function ladders(presets) {
  return (presets || []).filter((p) => p.ladder && p.ladder.length);
}

export const usable = (p) => p && p.available !== false;

/** "1920×1080", "30 fps", "6 Mb/s", as far as the request says. */
export function describe(request) {
  const r = request || {};
  const v = r.video || {};
  const out = {};
  if (r.no_video) out.size = "No picture";
  else if (v.width && v.height) out.size = `${v.width}×${v.height}`;
  else if (v.height) out.size = `${v.height}p`;
  if (!r.no_video && v.fps) out.fps = `${trim(v.fps.num / (v.fps.den || 1))} fps`;
  const kbps = r.no_video ? (r.audio || {}).bitrate_kbps : v.bitrate_kbps;
  if (kbps) out.bitrate = rate(kbps);
  if (v.codec && v.codec !== "h264") out.codec = v.codec.toUpperCase();
  return out;
}

export function rate(kbps) {
  return kbps >= 1000 ? `${trim(kbps / 1000)} Mb/s` : `${Math.round(kbps)} kb/s`;
}

function trim(n) {
  return String(Math.round(n * 100) / 100);
}

/**
 * Roughly what a request costs on the CPU, in millicores, for a preset that
 * came without a measured `cost`. 1080p30 H.264 at a fast preset is about
 * one and a half cores on the reference laptop; the rest scales by pixels.
 */
export function estimate(request) {
  const r = request || {};
  if (r.no_video) return 40;
  const v = r.video || {};
  const fps = v.fps ? v.fps.num / (v.fps.den || 1) : 30;
  const pixels = (v.width || 1920) * (v.height || 1080) * fps;
  const codec = { h265: 2, av1: 2.5, vp9: 2 }[v.codec] || 1;
  return Math.round((1500 * codec * pixels) / (1920 * 1080 * 30));
}

/**
 * The badge: free for a copy, light when it runs on a GPU or fits easily in
 * what is left, heavy when it would take a large bite of the CPU.
 */
export function costClass(preset, roomMillicores) {
  if (!preset || preset.id === COPY) return "free";
  const cost = preset.cost;
  if (cost && !cost.cpu_millicores && !cost.device_millis) return "free";
  if (cost && cost.device_millis && cost.cpu_millicores < 300) return "light";
  const cpu = cost ? cost.cpu_millicores : estimate(preset.request);
  const ceiling = roomMillicores ? Math.min(1200, roomMillicores / 2) : 1200;
  return cpu > ceiling ? "heavy" : "light";
}

export const COST_WORDS = { free: "Free", light: "Light", heavy: "Heavy" };

/**
 * True when copying the source already gives what the request asks for.
 * A field the request leaves out always matches; a bitrate matches within
 * the request's tolerance, a quarter either way when it names none.
 */
export function sourceMatches(shape, request) {
  if (!shape || !shape.video || !request) return false;
  const want = request.video || {};
  const have = shape.video;
  if (want.codec && want.codec !== have.codec) return false;
  if (want.width && want.width !== have.width) return false;
  if (want.height && want.height !== have.height) return false;
  if (want.fps && have.fps && Math.abs(fpsOf(want.fps) - fpsOf(have.fps)) > 0.05) return false;
  if (want.bitrate_kbps && have.bitrate_kbps) {
    const slack = want.bitrate_tolerance ?? 0.25;
    if (Math.abs(have.bitrate_kbps - want.bitrate_kbps) > want.bitrate_kbps * slack) return false;
  }
  return true;
}

const fpsOf = (f) => (typeof f === "number" ? f : f.num / (f.den || 1));

/**
 * Where the step starts: what the output already asks for on an edit, else
 * the platform's suggestion, unless the source already matches it, in which
 * case copy, because a copy of the right thing costs nothing.
 */
export function initialChoice(presets, platformId, shape, current) {
  if (current) return current.preset || "custom";
  const offered = singles(presets).filter(usable);
  const id = (SUGGEST[platformId] || []).find((s) => offered.some((p) => p.id === s));
  if (!id) return COPY;
  const preset = offered.find((p) => p.id === id);
  return sourceMatches(shape, preset.request) ? COPY : id;
}

/** The `rendition` field of an add or a set for a choice, or undefined for copy. */
export function renditionFor(choice, custom) {
  if (!choice || choice === COPY) return undefined;
  if (choice === "custom") return custom;
  return { preset: choice };
}
