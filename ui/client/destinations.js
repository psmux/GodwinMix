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
  {
    id: "udp",
    provides: ["udp/output"],
    title: "UDP or multicast",
    group: "Streams and servers",
    icon: "output",
    description: "MPEG-TS on a multicast group or to one receiver, for VLC, an IRD, a modulator or an IPTV network.",
    plugin: "udp",
    schema: {
      type: "object",
      required: ["id", "uri"],
      properties: {
        id: { type: "string", title: "Name", examples: ["lan"], description: "A short id. It appears in alerts and in the outputs list." },
        uri: { type: "string", title: "Address", examples: ["udp://239.1.1.1:5000"], description: "A multicast group or one receiver. rtp:// for a receiver that wants RTP." },
        ttl: { type: "integer", title: "TTL", default: 8, minimum: 1, maximum: 255, "x-gmx-group": "Advanced", description: "How many routers multicast may cross. 1 keeps it on this network." },
        interface: { type: "string", title: "Network interface", "x-gmx-group": "Advanced", description: "The interface multicast leaves by, such as eth1. Empty uses the default route." },
        cbr_kbps: { type: "integer", title: "Constant bitrate", minimum: 0, examples: ["Off (variable)"], "x-gmx-unit": "kbit", "x-gmx-group": "Advanced", description: "Pad with null packets to this rate, for hardware that needs one. At least 10% above the programme's bitrate." },
        queue_secs: QUEUE_FIELD,
      },
    },
    build: (v) => ({
      id: v.id,
      type: "udp/output",
      uri: v.uri,
      queue_secs: v.queue_secs ?? 4,
      ttl: v.ttl ?? 8,
      interface: v.interface || "",
      cbr_kbps: v.cbr_kbps ?? 0,
    }),
  },
  {
    id: "whep",
    provides: ["whep/output"],
    title: "WebRTC viewers (WHEP)",
    group: "Streams and servers",
    icon: "output",
    description: "Watch the programme in a browser or any WHEP player, under half a second behind, from this mixer's own address. No port of its own.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["id"],
      properties: {
        id: { type: "string", title: "Name", examples: ["monitor"], description: "A short id. Viewers open /whep/<this name>." },
        max_viewers: { type: "integer", title: "Most viewers", default: 10, minimum: 1, maximum: 500, description: "Each viewer is one more encrypted copy of the same encode. For a wide audience use HLS." },
        stun: { type: "string", title: "STUN server", default: "", examples: ["stun://stun.l.google.com:19302"], "x-gmx-group": "Advanced", description: "Empty for viewers on this network only. A STUN server lets viewers across the internet find this mixer." },
      },
    },
    build: (v) => ({ id: v.id, type: "whep/output", uri: "", max_viewers: v.max_viewers ?? 10, stun: v.stun ?? "" }),
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
    hint: "YouTube Studio, Go live: the stream key.",
    colour: "#ff0000",
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
    hint: "Live producer, Streaming software: the stream key.",
    colour: "#0866ff",
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
    hint: "Creator Dashboard, Settings, Stream: the primary key.",
    colour: "#9146ff",
  },
  // The five below were checked against each platform's own help pages in
  // September 2026. Instagram, LinkedIn and TikTok hand out a new address with
  // every stream, so their server box starts empty and says where to find it;
  // `perStream` is what tells a page to ask for both halves.
  {
    id: "instagram",
    title: "Instagram Live",
    provides: "rtmp/output",
    server: "",
    fixed: false,
    perStream: true,
    key: true,
    policy: "cdn",
    hosts: ["fbcdn.net"],
    example: "rtmps://edgetee-upload-xxx.xx.fbcdn.net:443/rtmp/",
    where:
      "Instagram Live Producer on a computer, Streaming software. Copy the stream URL and the " +
      "key; both change every time you go live.",
    hint: "Live Producer: URL and key, new each time.",
    colour: "#e1306c",
  },
  {
    id: "kick",
    title: "Kick",
    provides: "rtmp/output",
    server: "rtmps://fa723fc1b171.global-contribute.live-video.net:443/app",
    fixed: false,
    key: true,
    policy: "cdn",
    hosts: ["fa723fc1b171.global-contribute.live-video.net"],
    where:
      "Creator dashboard, Settings, Stream URL and Key. Copy the stream key, and the stream URL " +
      "too if it differs from the one filled in here.",
    hint: "Creator dashboard: Stream URL and Key.",
    colour: "#53fc18",
  },
  {
    id: "linkedin",
    title: "LinkedIn Live",
    provides: "rtmp/output",
    server: "",
    fixed: false,
    perStream: true,
    key: true,
    policy: "cdn",
    // LinkedIn does not publish its ingest host, so an existing output is not
    // recognised as LinkedIn and opens as Custom RTMP.
    hosts: [],
    example: "the stream URL from Live Studio",
    where:
      "Create the live event, then Live Studio, Manage streams, Get URL. Copy the stream URL " +
      "and the key; they appear up to two hours before the start.",
    hint: "Live Studio: stream URL and key, per event.",
    colour: "#0a66c2",
  },
  {
    id: "x",
    title: "X",
    provides: "rtmp/output",
    server: "rtmps://va.pscp.tv:443/x",
    fixed: false,
    key: true,
    policy: "cdn",
    hosts: ["pscp.tv"],
    where:
      "Media Studio Producer, Sources, your RTMP source. Copy the stream key, and the RTMPS URL " +
      "if yours is in another region.",
    hint: "Media Studio Producer: the RTMP source's key.",
    colour: "#000000",
  },
  {
    id: "tiktok",
    title: "TikTok LIVE",
    provides: "rtmp/output",
    server: "",
    fixed: false,
    perStream: true,
    key: true,
    policy: "cdn",
    hosts: ["tiktokcdn"],
    example: "rtmp://push-rtmp-xx.tiktokcdn.com/game/",
    where:
      "LIVE Producer on a computer, at livecenter.tiktok.com/producer. Copy the server URL and " +
      "the key. Only accounts TikTok has given encoder access see them.",
    hint: "LIVE Producer: server URL and key, per stream.",
    colour: "#000000",
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
    hint: "Any RTMP or RTMPS server.",
    colour: "#5b6b8c",
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
    hint: "A receiver, over SRT. No key.",
    colour: "#14a38b",
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
  // The longest host that matches wins: Kick and Twitch both ingest under
  // `contribute.live-video.net`, and Kick's is the more specific name.
  let known = null;
  let best = 0;
  for (const p of PLATFORMS) {
    for (const h of p.hosts) if (h.length > best && host.includes(h)) [known, best] = [p, h.length];
  }
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
