// The wall's Channels group, with no DOM: one item per channel stream (and
// one per channel nothing is publishing to), what each destination is doing,
// and what a filter keeps. The shape read is channel.list's
// (docs/reference/channels.md).

/** A destination is sending, or failing: retrying counts as failing, and so
 * does one still dialling for the first time that has been told why not. */
const SENDING = new Set(["live"]);
const FAILING = new Set(["failed", "reconnecting"]);
const failing = (d) => FAILING.has(d.state) || (d.state === "connecting" && !!d.error);

/** Whether destination `d` sends `stream`, `first` being the first live stream. */
const sends = (d, stream, first) => d.stream === stream || (d.stream === "*" && stream === first);

/** The destinations of a channel that are on and send this stream. */
function destinationsOf(channel, stream, first) {
  return (channel.destinations || []).filter((d) => d.enabled !== false && (stream == null || sends(d, stream, first)));
}

function counts(dests) {
  const bad = dests.filter(failing);
  return {
    total: dests.length,
    sending: dests.filter((d) => SENDING.has(d.state)).length,
    failing: bad.length,
    problems: bad.map((d) => `${d.label || d.id}: ${d.error || (d.state === "reconnecting" ? "retrying" : d.state)}`),
  };
}

const kbpsOf = (s) => ((s.video && s.video.kbps) || 0) + ((s.audio && s.audio.kbps) || 0);

/**
 * Every channel stream as a wall item, in channel then stream order.
 * @returns {Array<{kind: "channel", key: string, channel: string, title: string, stream: string|null, state: string}>}
 */
export function channelItems(list) {
  const out = [];
  const channels = ((list && list.channels) || []).slice().sort((a, b) => String(a.name || a.id).localeCompare(String(b.name || b.id)));
  for (const c of channels) {
    const streams = (c.streams || []).slice().sort((a, b) => a.name.localeCompare(b.name));
    const base = { kind: "channel", channel: c.id, title: c.name || c.id, enabled: c.enabled !== false };
    if (!streams.length) {
      out.push({ ...base, key: c.id, stream: null, state: c.enabled === false ? "off" : "waiting", kbps: 0, ...counts(destinationsOf(c, null)) });
      continue;
    }
    const first = (streams.find((s) => s.state === "live") || {}).name;
    for (const s of streams) {
      out.push({
        ...base,
        key: `${c.id}/${s.name}`,
        stream: s.name,
        state: s.state,
        protocol: s.protocol || "rtmp",
        video: s.video || null,
        kbps: s.state === "live" ? kbpsOf(s) : 0,
        source: s.source || null,
        ...counts(destinationsOf(c, s.name, first)),
      });
    }
  }
  return out;
}

/** "3 of 4 sending", or what there is instead. */
export function sendingText(it) {
  if (!it.total) return "No destinations";
  return `${it.sending} of ${it.total} sending`;
}

/** alarm, warning, ok or off, as the wall's health dot spells them. */
export function channelHealth(it) {
  if (it.failing) return "alarm";
  if (it.state === "live") return "ok";
  return it.state === "off" ? "off" : "warning";
}

/** "1920×1080 · 30 fps · H.264", from what the stream says. */
export function streamFormat(it) {
  const v = it.video;
  if (!v) return "";
  const parts = [];
  if (v.width) parts.push(`${v.width}×${v.height}`);
  if (v.fps) parts.push(`${Math.round(v.fps * 100) / 100} fps`);
  if (v.codec) parts.push(v.codec === "h264" ? "H.264" : v.codec === "h265" || v.codec === "hevc" ? "HEVC" : String(v.codec).toUpperCase());
  return parts.join(" · ");
}

/** What a filter keeps: the same words and alarm choice the shows obey. */
export function keepsChannel(it, text, alarm) {
  if (alarm === "any" && channelHealth(it) !== "alarm") return false;
  if (alarm && alarm !== "any" && !(alarm === "output-failed" && it.failing)) return false;
  const words = String(text || "").toLowerCase().split(/\s+/).filter(Boolean);
  if (!words.length) return true;
  const hay = [it.title, it.channel, it.stream, it.protocol, "channel", it.failing ? "failed" : "", ...(it.problems || [])].join(" ").toLowerCase();
  return words.every((w) => hay.includes(w));
}

/** The band and the items under it, or nothing when no channel is kept. */
export function channelGroup(items, opts = {}) {
  const kept = items.filter((it) => keepsChannel(it, opts.text, opts.alarm));
  if (!kept.length) return [];
  return [{ kind: "group", key: "channels", label: "Channels", count: kept.length }, ...kept];
}

/** Whether there is a picture to ask for. */
export const channelPictured = (it) => it.state === "live" && !!it.stream;
