// What is running, as a list a person can read and stop things from. No DOM
// here: a pure function of the status and the channel list, so the header,
// the What is running panel and the banner on a page that opens on a mixer
// already streaming all say the same thing, and the tests can say it too.
//
// A tester of 0.2.3 streamed to YouTube, closed the window, opened it again
// and found no way to stop the stream. Every item here carries the one call
// that stops it and, when it is live, the sentence that asks first.

const PLATFORMS = [
  [/youtube/i, "YouTube"],
  [/facebook|fbcdn/i, "Facebook"],
  [/twitch|live-video\.net/i, "Twitch"],
  [/kick/i, "Kick"],
  [/linkedin/i, "LinkedIn"],
  [/vimeo/i, "Vimeo"],
  [/restream/i, "Restream"],
];

/** "YouTube" for an address at youtube.com, else the destination's own name. */
export function platformName(host, fallback) {
  for (const [re, name] of PLATFORMS) if (re.test(String(host || ""))) return name;
  return fallback;
}

const dialling = (o) => (o.state === "connecting" || o.state === "reconnecting") && o.has_key !== false;

/** One programme output, or nothing when it is stopped or never dialled. */
function fromOutput(o, rates) {
  if (o.state === "stopped") return null;
  const kind = o.type === "record/output" ? "recording" : o.type === "hls/output" ? "watch" : "output";
  if (kind === "output" && o.state !== "live" && !dialling(o)) return null;
  const live = o.state === "live";
  const name = kind === "recording" ? "Recording" : kind === "watch" ? `Watch link ${o.id}` : platformName(o.uri_host, o.id);
  return {
    key: "output:" + o.id,
    kind,
    title: name,
    where: kind === "recording" ? "the programme, to a file" : "the programme",
    live,
    state: live ? "Live" : kind === "output" ? "Connecting" : "Starting",
    since_secs: kind === "recording" ? o.recording_secs : o.live_secs,
    kbps: rates && rates[o.id],
    stop: kind === "recording" ? { method: "output.remove", params: { id: o.id } } : { method: "output.stop", params: { id: o.id } },
  };
}

const SENDING = new Set(["live", "connecting", "reconnecting"]);

/** A channel's destinations that are on and sending, and its live encoders. */
function fromChannel(c, now) {
  const items = [];
  for (const d of c.destinations || []) {
    if (!d.enabled || !SENDING.has(d.state)) continue;
    const kind = d.platform === "hls" ? "watch" : d.platform === "file" ? "recording" : "send";
    const live = d.state === "live";
    items.push({
      key: `channel:${c.id}#${d.id}`,
      kind,
      title: kind === "watch" ? `Watch link ${d.label || d.id}` : kind === "recording" ? "Recording" : d.label || platformName(d.uri_host, d.id),
      where: `channel ${c.name || c.id}`,
      live,
      state: live ? "Live" : "Connecting",
      since_secs: live ? Math.floor((d.since_ms || 0) / 1000) : undefined,
      kbps: d.kbps || undefined,
      stop: { method: "channel.destination.set", params: { id: c.id, destination: d.id, enabled: false } },
    });
  }
  const streams = (c.streams || []).filter((s) => s.state === "live");
  if (streams.length && c.enabled !== false) {
    const first = Math.min(...streams.map((s) => s.since_ms || now));
    items.push({
      key: "ingest:" + c.id,
      kind: "ingest",
      title: c.name || c.id,
      where: streams.length === 1 ? `receiving from ${streams[0].from || "an encoder"}` : `receiving ${streams.length} streams`,
      live: true,
      state: "Receiving",
      since_secs: Math.max(0, Math.floor((now - first) / 1000)),
      kbps: undefined,
      stop: { method: "channel.set", params: { id: c.id, enabled: false } },
    });
  }
  return items;
}

/**
 * Everything running, programme first.
 * @param {object} status   the mixer status, with `outputs`
 * @param {object[]} [channels]  `channel.list`'s channels, when read
 * @param {object} [rates]  kb/s by output id, measured by whoever polls
 * @param {number} [now]    ms since 1970, for an encoder's since_ms
 */
export function runningThings(status, channels, rates, now = Date.now()) {
  const items = ((status && status.outputs) || []).map((o) => fromOutput(o, rates)).filter(Boolean);
  for (const c of channels || []) items.push(...fromChannel(c, now));
  // Streams before recordings, whatever order the status lists them in.
  const rank = (i) => ["output", "watch", "send", "recording", "ingest"].indexOf(i.kind);
  return items.sort((a, b) => rank(a) - rank(b));
}

/** What goes out: streams, watch links and recordings, not encoders coming in. */
export const outgoing = (items) => items.filter((i) => i.kind !== "ingest");

/** The question asked before a live thing is stopped, or null when it is not live. */
export function confirmWording(item) {
  if (!item.live) return null;
  switch (item.kind) {
    case "recording":
      return { title: "Stop recording?", body: "The file is finished and kept on the mixer.", yes: "Stop recording" };
    case "watch":
      return { title: `Stop the ${item.title.toLowerCase()}?`, body: "Anyone watching sees it end.", yes: "Stop" };
    case "ingest":
      return {
        title: `Turn away the encoders sending to ${item.title}?`,
        body: "They are disconnected, and refused until the channel is switched on again.",
        yes: "Turn them away",
      };
    default:
      return { title: `Stop streaming to ${item.title}?`, body: "Viewers see the stream end.", yes: "Stop streaming" };
  }
}

/** The label of the button that stops every outgoing thing at once. */
export function stopAllLabel(items) {
  return outgoing(items).some((i) => i.kind === "recording") ? "Stop all streaming and recording" : "Stop all streaming";
}

/** The one question Stop all asks, naming what it stops. */
export function stopAllWording(items) {
  const out = outgoing(items);
  const names = [...new Set(out.filter((i) => i.kind !== "recording").map((i) => i.title))];
  const parts = [];
  if (names.length) parts.push(`Viewers of ${listed(names)} see the stream end.`);
  if (out.some((i) => i.kind === "recording")) parts.push("The recording is finished and kept on the mixer.");
  parts.push("Each destination keeps its stream key, so Start streaming sends to it again.");
  return { title: stopAllLabel(items) + "?", body: parts.join(" "), yes: stopAllLabel(items) };
}

/** "YouTube, Facebook and Twitch". */
export function listed(names) {
  if (names.length < 2) return names.join("");
  return names.slice(0, -1).join(", ") + " and " + names[names.length - 1];
}

/** "YouTube for 1:56:23, recording": the banner's and the tray's one line. */
export function summary(items, fmt) {
  return items
    .map((i) => {
      const name = i.kind === "recording" ? "recording" : i.kind === "ingest" ? `${i.title} receiving` : i.title;
      return i.live && i.since_secs !== undefined ? `${name} for ${fmt(i.since_secs)}` : name;
    })
    .join(", ");
}

/** The longest anything has been live, for the header's clock. */
export function longestLive(items) {
  const secs = items.filter((i) => i.live && i.since_secs !== undefined).map((i) => i.since_secs);
  return secs.length ? Math.max(...secs) : undefined;
}
