// What the plan did and what the governor measured, in sentences.
//
// Pure, like model.js. The shapes are `rendition.plan`, `governor.status` and
// the `data` of a refusal, as dev/plans/wave2-contract.md has them.

/** Encoders that run on the CPU, by the catalogue's ids. Anything else is hardware. */
const SOFTWARE = /^(x264|x265|openh264|svt|rav1e|vp8|vp9|libvpx|aac|avenc|fdk|opus|lame|mp3)/;

/** Reasons that are the planner's first choice, and need no "because". */
const QUIET = new Set(["", "ok", "preferred", "best", "fits", "copy", "shared"]);

/** An encoder as the plan names it: a catalogue id, or an EncoderSlot. */
export function encoderOf(node) {
  const e = node && node.encoder;
  if (!e) return null;
  if (typeof e === "string") return { id: e, hardware: !SOFTWARE.test(e) };
  return { id: e.id, hardware: e.hardware === undefined ? !SOFTWARE.test(e.id || "") : !!e.hardware, device: e.device };
}

const isAudio = (n) => /audio/.test(n.kind || "") || /^(aac|opus|mp3|avenc_aac|fdk|lame)/.test((encoderOf(n) || {}).id || "");

/**
 * The nodes of a plan that serve one output, the video encode first. The
 * plan names requests; an output's request carries its own id, so the two
 * are the same slug.
 */
export function nodesFor(plan, id) {
  const nodes = ((plan && plan.nodes) || []).filter((n) => (n.serves || []).includes(id));
  const encodes = nodes.filter((n) => encoderOf(n));
  return { nodes, video: encodes.find((n) => !isAudio(n)), audio: encodes.find(isAudio) };
}

/**
 * One line for an output row: "Copied, no re-encoding", "Encoded on the GPU
 * (h264-videotoolbox), shared with 2 others", "Encoded on the CPU (x264)
 * because the GPU is full". Empty when the plan does not mention it.
 */
export function planLine(plan, id) {
  const { nodes, video, audio } = nodesFor(plan, id);
  if (!nodes.length) return "";
  const node = video || audio;
  if (!node) return "Copied, no re-encoding";
  const enc = encoderOf(node);
  const what = video ? "Encoded" : "Sound encoded";
  let line = `${what} on the ${enc.hardware ? "GPU" : "CPU"} (${enc.id})`;
  const others = (node.serves || []).length - 1;
  if (others > 0) line += `, shared with ${others} other${others === 1 ? "" : "s"}`;
  const why = because(node);
  if (why) line += `${others > 0 ? "," : ""} because ${why}`;
  return line;
}

/** The same, short enough for a destination tile. */
export function planShort(plan, id) {
  const { nodes, video, audio } = nodesFor(plan, id);
  if (!nodes.length) return "";
  const node = video || audio;
  if (!node) return "Copied";
  const enc = encoderOf(node);
  const others = (node.serves || []).length - 1;
  return `${enc.hardware ? "GPU" : "CPU"} ${enc.id}${others > 0 ? `, shared` : ""}`;
}

/** The reason as a clause, "the GPU is full", or "" when it was the first choice. */
function because(node) {
  const r = node.reason || {};
  if (QUIET.has(r.code || "") || !r.text) return "";
  const t = String(r.text).trim().replace(/\.$/, "");
  return t.charAt(0).toLowerCase() + t.slice(1);
}

/** Millicores as a person says them: "half a core", "1.8 cores". */
export function cores(m) {
  if (!m) return "none";
  if (m < 1000) return `${Math.max(1, Math.round(m / 10))}% of one core`;
  const n = Math.round(m / 100) / 10;
  return `${n} core${n === 1 ? "" : "s"}`;
}

function sessions(n) {
  return n === 1 ? "one GPU encoder session" : `${n} GPU encoder sessions`;
}

/** The parts of a Cost worth saying, as phrases. */
export function costPhrases(cost) {
  const c = cost || {};
  const out = [];
  if (c.cpu_millicores) out.push(`${cores(c.cpu_millicores)} of the CPU`);
  if (c.device_millis) out.push(`${Math.round(c.device_millis / 10)}% of the GPU`);
  if (c.device_sessions) out.push(sessions(c.device_sessions));
  if (c.egress_kbps) out.push(`${upload(c.egress_kbps)} of upload`);
  return out;
}

export function upload(kbps) {
  return kbps >= 1000 ? `${Math.round(kbps / 100) / 10} Mb/s` : `${Math.round(kbps)} kb/s`;
}

/** "a, b and c". */
export function list(parts) {
  if (parts.length < 2) return parts[0] || "";
  return parts.slice(0, -1).join(", ") + " and " + parts.at(-1);
}

/**
 * A governor refusal's `data` as two sentences: what it needs, and what is
 * left. Only the resources the request uses are named on either side.
 */
export function refusalWords(data) {
  const need = (data && data.need) || {};
  const have = (data && data.have) || {};
  const needs = costPhrases(need);
  const room = [];
  if (need.cpu_millicores) room.push(have.cpu_millicores ? `${cores(have.cpu_millicores)} of the CPU` : "no CPU to spare");
  if (need.device_millis) room.push(have.device_millis ? `${Math.round(have.device_millis / 10)}% of the GPU` : "no GPU time");
  if (need.device_sessions) room.push(have.device_sessions ? sessions(have.device_sessions) : "no GPU sessions");
  if (need.egress_kbps) room.push(`${upload(have.egress_kbps || 0)} of upload`);
  return {
    need: needs.length ? `This needs about ${list(needs)}.` : "This needs more than the machine has left.",
    room: room.length ? `Without dropping what is on air, there is room for ${list(room)}.` : "",
  };
}

/** A refusal from the governor, as against any other kind of failure. */
export function isRefusal(err) {
  const d = err && err.data;
  return !!(d && d.need && d.have);
}

/** "3 minutes ago", "on 12 September", for when the machine was measured. */
export function measuredWhen(iso, now = Date.now()) {
  const t = Date.parse(iso || "");
  if (!Number.isFinite(t)) return "Not measured yet";
  const s = Math.max(0, (now - t) / 1000);
  if (s < 90) return "Measured just now";
  if (s < 5400) return `Measured ${Math.round(s / 60)} minutes ago`;
  if (s < 129600) return `Measured ${Math.round(s / 3600)} hours ago`;
  if (s < 30 * 86400) return `Measured ${Math.round(s / 86400)} days ago`;
  return `Measured on ${new Date(t).toLocaleDateString(undefined, { day: "numeric", month: "long" })}`;
}
