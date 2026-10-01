// The wall's arithmetic, with no DOM: words for numbers, the order of the
// rows, what a filter keeps and the counts in the header. Shapes are
// dev/plans/wave4-contract.md (Show, Health, InputStats, OutputStats).

export const ALARMS = {
  "no-input": "No input", stall: "Stalled", black: "Black", freeze: "Frozen", silence: "Silent",
  "cc-errors": "CC errors", loss: "Loss", "output-failed": "Output failed", "governor-refused": "Refused", shed: "Shed",
};

const RANK = { alarm: 3, warning: 2, ok: 1, off: 0 };
export const GROUPS = [["alarm", "In alarm"], ["warning", "Warning"], ["ok", "Running"], ["off", "Off"]];

const SCHEMES = [
  [/^udp:\/\/@?2(2[4-9]|3\d)\./, "Multicast"], [/^udp:/, "UDP"], [/^srt:/, "SRT"], [/^rtmps?:/, "RTMP"],
  [/^rtsp:/, "RTSP"], [/^rist:/, "RIST"], [/\.m3u8(\?|$)/, "HLS"], [/^https?:/, "HTTP"], [/^file:/, "File"], [/^channel:/, "Channel"],
];

/** The transport an input arrives on, from its address. */
export function transport(uri) {
  const u = String(uri || "").toLowerCase();
  const hit = SCHEMES.find(([re]) => re.test(u));
  return hit ? hit[1] : u ? u.split(":")[0].toUpperCase() : "";
}

export function kbps(k) {
  if (!k && k !== 0) return "";
  if (k >= 1000000) return `${(k / 1000000).toFixed(2)} Gb/s`;
  if (k >= 10000) return `${Math.round(k / 1000)} Mb/s`;
  if (k >= 1000) return `${(k / 1000).toFixed(1)} Mb/s`;
  return `${Math.round(k)} kb/s`;
}

/** How long ago, in the fewest words a glance needs. */
export function age(sinceMs, now = Date.now()) {
  const s = Math.max(0, Math.round((now - sinceMs) / 1000));
  if (s < 60) return `${s} s`;
  if (s < 3600) return `${Math.floor(s / 60)} min`;
  if (s < 86400) return `${Math.floor(s / 3600)} h ${Math.floor((s % 3600) / 60)} min`;
  return `${Math.floor(s / 86400)} d`;
}

/** "1920×1080 · 25 fps · H.264", from what is known. */
export function format(i) {
  if (!i) return "";
  const parts = [];
  if (i.width) parts.push(`${i.width}×${i.height}`);
  if (i.fps) parts.push(`${Math.round(i.fps * 100) / 100} fps`);
  if (i.video_codec) parts.push(codec(i.video_codec));
  return parts.join(" · ");
}

const CODECS = { h264: "H.264", h265: "HEVC", hevc: "HEVC", mpeg2: "MPEG-2", av1: "AV1", vp9: "VP9", aac: "AAC", mp2: "MP2", ac3: "AC-3", opus: "Opus" };
export const codec = (c) => CODECS[String(c).toLowerCase()] || String(c || "").toUpperCase();

/** A show from before wave 4 has no `compositing`: every one of those mixes. */
export const mixed = (show) => !!show && show.compositing !== false;
export const healthOf = (show) => (show && show.health) || { state: show && show.state === "stopped" ? "off" : "ok", alarms: [] };
export const rank = (show) => RANK[healthOf(show).state] ?? 1;
export const isLive = (show) => show.state === "running" || show.state === "live";
export const load = (stats) => ((stats && stats.outputs) || []).reduce((n, o) => n + (o.cpu_millicores || 0), 0);
export const inKbps = (stats) => (stats && stats.input && stats.input.kbps) || 0;

/** The text a filter looks through: names, addresses and alarm words. */
function haystack(show) {
  const outs = (show.outputs || []).map((o) => `${o.id} ${o.uri || o.platform || ""}`);
  const alarms = healthOf(show).alarms.map((a) => ALARMS[a.kind] || a.kind);
  return [show.name, show.id, show.input && show.input.uri, transport(show.input && show.input.uri), ...outs, ...alarms].join(" ").toLowerCase();
}

/** `alarm` is "" for every show, "any" for alarm or warning, or one alarm kind. */
export function keeps(show, text, alarm) {
  if (alarm === "any" && rank(show) < 2) return false;
  if (alarm && alarm !== "any" && !healthOf(show).alarms.some((a) => a.kind === alarm)) return false;
  const words = String(text || "").toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const hay = haystack(show);
  return words.every((w) => hay.includes(w));
}

const oldest = (show) => Math.min(...healthOf(show).alarms.map((a) => a.since_ms), Infinity);

/** Each column's sort key. Numbers sort largest first on the first click. */
export const SORTS = {
  name: { key: (s) => s.name.toLowerCase() },
  state: { key: (s) => -rank(s) },
  input: { key: (s) => `${transport(s.input && s.input.uri)} ${s.input ? s.input.uri : ""}` },
  kbps: { key: (s, st) => -inKbps(st) },
  outputs: { key: (s) => -(s.outputs || []).filter((o) => o.state === "failed" || o.error).length * 1000 - (s.outputs || []).length },
  load: { key: (s, st) => -load(st) },
  alarms: { key: (s) => [-rank(s), oldest(s)] },
  mixing: { key: (s) => (mixed(s) ? 0 : 1) },
};

function compare(a, b) {
  if (Array.isArray(a)) {
    for (let i = 0; i < a.length; i++) { const c = compare(a[i], b[i]); if (c) return c; }
    return 0;
  }
  return a < b ? -1 : a > b ? 1 : 0;
}

/**
 * The rows to draw, in order, with a band before each group when grouped.
 * @returns {Array<{kind: "group", key: string, label: string, count: number} | {kind: "show", show: object}>}
 */
export function rows(shows, stats, opts = {}) {
  const sort = SORTS[opts.sort] || SORTS.alarms;
  const dir = opts.dir === -1 ? -1 : 1;
  const kept = shows.filter((s) => keeps(s, opts.text, opts.alarm));
  const keyed = kept.map((s) => [sort.key(s, stats.get(s.id)), s]);
  keyed.sort((x, y) => dir * compare(x[0], y[0]) || compare(x[1].name, y[1].name));
  const ordered = keyed.map(([, s]) => s);
  if (!opts.group) return ordered.map((show) => ({ kind: "show", show }));
  const out = [];
  for (const [key, label] of GROUPS) {
    const these = ordered.filter((s) => healthOf(s).state === key);
    if (!these.length) continue;
    out.push({ kind: "group", key, label, count: these.length });
    for (const show of these) out.push({ kind: "show", show });
  }
  return out;
}

/** The header's counts. `gov` is governor.status, or null. */
export function summary(shows, stats, gov) {
  const live = shows.filter(isLive).length;
  const alarm = shows.filter((s) => healthOf(s).state === "alarm").length;
  let inK = gov && gov.ingress_kbps;
  let outK = gov && gov.egress_kbps;
  const partial = inK == null;
  if (inK == null) inK = [...stats.values()].reduce((n, st) => n + inKbps(st), 0);
  if (outK == null) outK = [...stats.values()].reduce((n, st) => n + (st.outputs || []).reduce((m, o) => m + (o.kbps || 0), 0), 0);
  const cpu = gov && gov.cpu && gov.cpu.cores ? Math.round(gov.cpu.used_millicores / gov.cpu.cores / 10) : null;
  const dev = gov && (gov.devices || [])[0];
  const gpu = dev ? Math.round((dev.used_millis || 0) / 10) : null;
  return { shows: shows.length, live, alarm, inK, outK, partial, cpu, gpu };
}
