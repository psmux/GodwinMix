// What feeds a channel, in words a person reads at a glance. A channel gets
// its picture from this mixer's programme, from an encoder publishing to it,
// or from an address it pulls, and sends it on to its push destinations, the
// way Livebox has it. No DOM here, so the words are testable with a plain
// object.

import { isLive, fmtKbps, codecName } from "./model.js";

export const KINDS = [
  { kind: "programme", label: "This mixer's programme" },
  { kind: "encoder", label: "An encoder (OBS, a phone, a camera)" },
  { kind: "pull", label: "Pull from a URL" },
];

/** programme, encoder or pull. A channel from an older core is an encoder's. */
export function feedKind(channel) {
  return (channel.feed && channel.feed.kind) || "encoder";
}

/** "Video 1920x1080 3.0 Mb/s | Audio AAC 128 kb/s", leaving out what is not known. */
export function specLine(s) {
  const parts = [];
  if (s.video) {
    const size = s.video.width ? `${s.video.width}x${s.video.height}` : codecName(s.video.codec);
    parts.push(["Video", size, s.video.kbps ? fmtKbps(s.video.kbps) : ""].filter(Boolean).join(" "));
  }
  if (s.audio) parts.push(["Audio", codecName(s.audio.codec), s.audio.kbps ? fmtKbps(s.audio.kbps) : ""].filter(Boolean).join(" "));
  return parts.join(" | ");
}

/** Why a channel shows no picture, in plain words. */
export function offlineReason(channel) {
  const f = channel.feed || {};
  const said = f.error ? `: ${f.error}` : "";
  switch (feedKind(channel)) {
    case "programme":
      return `waiting for this mixer's programme${said}`;
    case "pull": {
      const from = f.uri_host || "the URL";
      if (f.state === "reconnecting") {
        const s = f.retry_in_ms ? ` in ${Math.max(1, Math.round(f.retry_in_ms / 1000))} s` : "";
        return `pulling ${from}, trying again${s}${said}`;
      }
      return `pulling ${from}, connecting${said}`;
    }
    default:
      return "waiting for an encoder";
  }
}

/** The status line: `{ state, label, text }`, state being live, offline or off. */
export function statusOf(channel) {
  if (!channel.enabled) return { state: "off", label: "Off", text: "Switched off" };
  if (isLive(channel)) {
    const first = (channel.streams || []).find((s) => s.state === "live");
    return { state: "live", label: "LIVE", text: specLine(first) };
  }
  return { state: "offline", label: "Offline", text: offlineReason(channel) };
}

/** The small marks a row carries, as Livebox shows them: pull, push, record, convert. */
export function badgesOf(channel) {
  const on = (channel.destinations || []).filter((d) => d.enabled);
  const out = [];
  if (feedKind(channel) === "programme") out.push({ id: "programme", title: "Fed by this mixer's programme" });
  if (feedKind(channel) === "pull") out.push({ id: "pull", title: `Pulls ${(channel.feed && channel.feed.uri_host) || "a URL"}` });
  if (on.some((d) => d.platform !== "file" && d.platform !== "hls")) out.push({ id: "push", title: "Sends on to push destinations" });
  if (on.some((d) => d.platform === "file")) out.push({ id: "record", title: "Records to a file" });
  if (on.some((d) => d.rendition && d.rendition.preset !== "copy")) out.push({ id: "convert", title: "Converts for a destination's own format" });
  return out;
}

/** The `channel.set` params for a new feed, or the sentence saying what is missing. */
export function feedParams(id, kind, url = "") {
  if (kind !== "pull") return { params: { id, feed: { kind } } };
  const u = String(url).trim();
  if (!/^(rtmps?|srt|rtsps?|https?):\/\/[^/?:]/i.test(u)) {
    return { error: "Paste the whole address, starting rtmp://, srt://, rtsp:// or https:// for HLS." };
  }
  return { params: { id, feed: { kind, url: u } } };
}

/** The toast once a platform is added, by what feeds the channel. */
export function addedWords(title, channel, fed = false) {
  if (isLive(channel)) return `${title} added. It goes live with ${channel.name}.`;
  if (fed || feedKind(channel) === "programme") return `${title} added. It gets this mixer's programme once it reaches ${channel.name}.`;
  if (feedKind(channel) === "pull") return `${title} added. It goes live once ${channel.name} has the stream it pulls.`;
  return `${title} added. It waits for an encoder to send to ${channel.name}; nothing reaches ${title} until one does.`;
}
