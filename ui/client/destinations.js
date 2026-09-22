// Where the programme can be sent: the output kinds the picker offers and the
// platform table the destination form is built from.
//
// Out of `kinds.js`, which every page loads, because only the destination
// form and the output picker read these, and both are loaded on demand.

/**
 * The two fields every destination form has, written once. Both are jargon
 * without a hint: "own" and "cdn" are the protocol's words and a buffer in
 * seconds says nothing about what it is for. The labels name what is being
 * chosen; the values stay as the wire has them.
 */
const POLICY_FIELD = {
  type: "string",
  title: "When it drops",
  enum: ["own", "cdn"],
  "x-gmx-labels": ["Retry quickly (a server you run)", "Back off (a platform that penalises hammering)"],
  default: "own",
  description:
    "own retries quickly, for a server you run; cdn backs off harder, for a platform that " +
    "penalises hammering.",
};

const QUEUE_FIELD = {
  type: "number",
  title: "Outage buffer",
  default: 4,
  minimum: 0,
  maximum: 60,
  "x-gmx-unit": "s",
  "x-gmx-group": "Advanced",
  description: "How much encoded video to hold, so a short drop is invisible to the viewer.",
};

export const OUTPUT_KINDS = [
  {
    id: "rtmp",
    provides: ["rtmp/output"],
    title: "RTMP destination",
    group: "Streams and servers",
    icon: "output",
    description: "YouTube, Facebook, Twitch, or your own server.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["id", "uri"],
      properties: {
        id: { type: "string", title: "Name", examples: ["youtube"], description: "A short id. It appears in alerts and in the outputs list." },
        uri: { type: "string", title: "Address and key", examples: ["rtmp://a.rtmp.youtube.com/live2/xxxx-xxxx"] },
        policy: POLICY_FIELD,
        queue_secs: QUEUE_FIELD,
      },
    },
    build: (v) => ({ id: v.id, uri: v.uri, policy: v.policy || "own", queue_secs: v.queue_secs ?? 4 }),
  },
  {
    id: "srt",
    provides: ["srt/output"],
    title: "SRT destination",
    group: "Streams and servers",
    icon: "output",
    description: "MPEG-TS over SRT, to a receiver that expects it. No stream key.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["id", "uri"],
      properties: {
        id: { type: "string", title: "Name", examples: ["studio"], description: "A short id. It appears in alerts and in the outputs list." },
        uri: { type: "string", title: "Address", examples: ["srt://192.168.1.50:9000"], description: "Caller mode unless the address says otherwise." },
        latency_ms: { type: "integer", title: "Receive buffer", default: 125, minimum: 0, maximum: 10000, "x-gmx-unit": "ms", "x-gmx-group": "Advanced", description: "How much the receiver is asked to hold, which is what an SRT link trades for a lossy network." },
        policy: POLICY_FIELD,
        queue_secs: QUEUE_FIELD,
      },
    },
    build: (v) => ({
      id: v.id,
      uri: v.uri,
      policy: v.policy || "own",
      queue_secs: v.queue_secs ?? 4,
      latency_ms: v.latency_ms ?? 125,
    }),
  },
];

/**
 * Where a volunteer actually sends a service, and the address that gets it
 * there. The point of this table is that nobody has to know what an RTMP URL
 * is: they pick the platform they were told to use and paste the key.
 *
 * `server` is the ingest, with no trailing slash; the key is joined onto it.
 * These are the published defaults and they are stable enough to ship, but
 * they are a convenience and not a contract: `custom` exists for anyone whose
 * platform is not here or who was given a different server, and every one of
 * them stays editable in the form.
 *
 * `hosts` is what an existing output is recognised by. `uri_host` is all a
 * client ever gets back, which is enough to name the platform and nothing
 * like enough to reconstruct the key.
 */
export const PLATFORMS = [
  {
    id: "youtube",
    title: "YouTube",
    provides: "rtmp/output",
    server: "rtmp://a.rtmp.youtube.com/live2",
    fixed: true,
    key: true,
    policy: "cdn",
    hosts: ["rtmp.youtube.com"],
    where: "YouTube Studio, Go live, Stream settings. Copy the stream key, not the stream URL.",
  },
  {
    id: "facebook",
    title: "Facebook",
    provides: "rtmp/output",
    server: "rtmps://live-api-s.facebook.com:443/rtmp",
    fixed: true,
    key: true,
    policy: "cdn",
    hosts: ["live-api-s.facebook.com"],
    where: "The Live producer page, Streaming software. Copy the stream key.",
  },
  {
    id: "twitch",
    title: "Twitch",
    provides: "rtmp/output",
    server: "rtmp://live.twitch.tv/app",
    fixed: true,
    key: true,
    policy: "cdn",
    hosts: ["live.twitch.tv", "contribute.live-video.net"],
    where:
      "The Creator Dashboard, Settings, Stream. Copy the primary stream key. Twitch also " +
      "publishes ingest servers nearer to you; this one works everywhere and you can paste " +
      "a closer one over it.",
  },
  {
    id: "custom",
    title: "Custom RTMP",
    provides: "rtmp/output",
    server: "",
    fixed: false,
    key: true,
    policy: "own",
    hosts: [],
    // Said in `where`, and the form has to mean it: a whole address pasted
    // into the server box, with nothing in the key box, is a whole address.
    keyOptional: true,
    where: "Your own server, or a platform that is not on this list. The key may be blank.",
  },
  {
    id: "srt",
    title: "SRT",
    provides: "srt/output",
    server: "",
    fixed: false,
    key: false,
    policy: "own",
    hosts: [],
    // What the empty address box shows. Without it the form fell back to the
    // RTMP example, which is the wrong kind of address for this tile.
    example: "srt://192.168.1.50:9000",
    where: "A receiver that expects MPEG-TS over SRT. There is no stream key.",
  },
];

/** The platform for a slug, or undefined. */
export function platform(id) {
  return PLATFORMS.find((p) => p.id === id);
}

/**
 * Which platform an existing output is on, read from the masked host the core
 * hands out. Falls back to `srt` for an srt:// address and `custom` for
 * anything else, so the edit form always has something to open with.
 */
export function platformOfHost(uriHost) {
  const host = String(uriHost || "").toLowerCase();
  const known = PLATFORMS.find((p) => p.hosts.some((h) => host.includes(h)));
  if (known) return known;
  return platform(host.startsWith("srt://") ? "srt" : "custom");
}

/**
 * Server plus key, the way every RTMP ingest on the table wants it. The key is
 * trimmed: pasting one out of a platform's dashboard picks up a newline often
 * enough that not trimming it is a support ticket.
 */
export function joinKey(server, key) {
  const base = String(server || "").trim().replace(/\/+$/, "");
  const secret = String(key || "").trim();
  if (!secret) return base;
  return base + "/" + secret;
}

/** Kind to default tile colour, as a CSS custom property name. */
