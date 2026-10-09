// What one line of the Rows view says about a channel, worked out from the
// answer to `channel.list` and nothing else. No DOM here, so the words are
// testable with a plain object.

import { Channels, isLive, liveCount, resolution, fmtFps, fmtKbps, streamKbps, startedAt, ringState } from "./model.js";

/** Every channel in a `channel.list` answer as row data, in the panel's order. */
export function rowsOf(answer, now = Date.now()) {
  const model = new Channels();
  model.load(answer);
  return model.list().map((c) => rowOf(c, now));
}

/**
 * One channel as a row: which dot, what it is doing, the first live stream's
 * picture and bit rate, when it began, and how many destinations are sending.
 */
export function rowOf(channel, now = Date.now()) {
  const live = isLive(channel);
  const count = liveCount(channel);
  const first = (channel.streams || []).find((s) => s.state === "live");
  const dests = channel.destinations || [];
  const sending = dests.filter((d) => d.enabled && d.state === "live").length;
  return {
    id: channel.id,
    name: channel.name || channel.id,
    state: !channel.enabled ? "off" : live ? "live" : "waiting",
    status: !channel.enabled ? "Switched off" : live ? (count > 1 ? `Live, ${count} streams` : "Live") : "Waiting for an encoder",
    spec: first ? specOf(first) : "",
    began: first ? startedAt(first.since_ms, now) : 0,
    sending: dests.length ? `${sending} of ${dests.length} sending` : "No push destinations",
    rings: dests.map((d) => ({ id: d.id, label: d.label || d.platform, state: ringState(d) })),
  };
}

/** "1920×1080 30 fps 4.5 Mb/s", leaving out what the encoder did not say. */
export function specOf(s) {
  // A measured rate wanders (29.05, 30.51); one line wants the whole number.
  const parts = [resolution(s), s.video && s.video.fps ? fmtFps(Math.round(s.video.fps)) : "", fmtKbps(streamKbps(s))];
  return parts.filter(Boolean).join(" ");
}
