// What you can add, and what it needs.
//
// The core says which kinds exist (`core.api` `kinds`); this table says how
// each one looks and what its form asks for. Icons are inline SVG path data
// rather than files: a picker that costs one request per tile stutters on a Pi.

export const ICONS = {
  camera: "M4 7h3l2-2h6l2 2h3v11H4V7zm8 3a3.5 3.5 0 100 7 3.5 3.5 0 000-7z",
  file: "M6 3h8l4 4v14H6V3zm8 1.5V8h3.5L14 4.5zM8 12h8v1.5H8V12zm0 3.5h8V17H8v-1.5z",
  page: "M3 5h18v14H3V5zm0 4h18M7 7V5m-4 2h18",
  stream: "M12 12a3 3 0 100 .01M6.5 6.5a8 8 0 000 11M17.5 6.5a8 8 0 010 11M3.5 3.5a12 12 0 000 17M20.5 3.5a12 12 0 010 17",
  exec: "M4 4h16v16H4V4zm3 4l4 4-4 4m6 1h5",
  graphic: "M4 4h16v12H4V4zm0 16h16M9 9l3 3 3-3",
  output: "M4 6h10v12H4V6zm12 3l5-3v12l-5-3V9z",
  media: "M5 4l14 8-14 8V4z",
  device: "M8 3h8v18H8V3zm3 16h2",
};

/**
 * @typedef {{id, title, group, icon, description, plugin, schema, build}} Kind
 * `build(values)` turns the form's answers into the params a call wants.
 */

export const SOURCE_KINDS = [
  {
    id: "file",
    provides: ["file/source"],
    title: "Video file",
    group: "Files and pages",
    icon: "file",
    description: "A clip on this machine. Scrubs and loops.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", title: "Path or file:// URL", examples: ["/srv/media/opener.mp4"] },
        name: { type: "string", title: "Name", description: "What the tile says. Taken from the file name when left blank." },
      },
    },
    build: (v) => ({ uri: v.uri, name: v.name || null, kind: null, superimpose: null }),
  },
  {
    id: "page",
    provides: ["browser/source", "layered/source"],
    title: "Web page",
    group: "Files and pages",
    icon: "page",
    description: "A URL rendered by the browser sidecar. Lyrics, scoreboards, lower thirds.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", format: "uri", title: "Address", examples: ["https://example.org/lyrics"] },
        name: { type: "string", title: "Name" },
        superimpose: {
          type: "string",
          title: "Video inside the page",
          enum: ["off", "auto"],
          default: "off",
          description: "off: the browser renders everything. auto: the mixer decodes the page's own video and draws the page over it, which is sharper and cheaper when the page is mostly one video.",
        },
      },
    },
    build: (v) => ({ uri: v.uri, name: v.name || null, kind: "web", superimpose: v.superimpose || "off" }),
  },
  {
    id: "stream",
    provides: ["rtmp/source", "hls/source"],
    title: "Incoming stream",
    group: "Streams and servers",
    icon: "stream",
    description: "RTMP, SRT, RTSP or HLS pulled from somewhere else.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", title: "Address", examples: ["rtmp://192.168.1.20/live/cam1"] },
        name: { type: "string", title: "Name" },
      },
    },
    build: (v) => ({ uri: v.uri, name: v.name || null, kind: null, superimpose: null }),
  },
  {
    id: "exec",
    provides: ["exec/source"],
    title: "Command",
    group: "Everything else",
    icon: "exec",
    description: "A program that writes frames to its output. Off unless the config allows it.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", title: "Command line", examples: ["ffmpeg -re -i input.mp4 -f mpegts -"] },
        name: { type: "string", title: "Name" },
      },
    },
    build: (v) => ({
      uri: v.uri.startsWith("exec:") ? v.uri : "exec:" + v.uri,
      name: v.name || null,
      kind: null,
      superimpose: null,
    }),
  },
];

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
        policy: {
          type: "string",
          title: "When it drops",
          enum: ["own", "cdn"],
          default: "own",
          description: "own: reconnect on our schedule, for a server you run. cdn: back off the way the big platforms want.",
        },
        queue_secs: { type: "number", title: "Outage buffer", default: 4, minimum: 0, maximum: 60, "x-gmx-unit": "s", "x-gmx-group": "Advanced" },
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
        latency_ms: { type: "integer", title: "Receive buffer", default: 125, minimum: 0, maximum: 10000, "x-gmx-unit": "ms", "x-gmx-group": "Advanced" },
        policy: {
          type: "string",
          title: "When it drops",
          enum: ["own", "cdn"],
          default: "own",
          description: "own: reconnect on our schedule, for a server you run. cdn: back off the way the big platforms want.",
        },
        queue_secs: { type: "number", title: "Outage buffer", default: 4, minimum: 0, maximum: 60, "x-gmx-unit": "s", "x-gmx-group": "Advanced" },
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
export const KIND_COLOUR = {
  file: "var(--kind-file)",
  page: "var(--kind-page)",
  stream: "var(--kind-stream)",
  exec: "var(--kind-other)",
  camera: "var(--kind-camera)",
  graphic: "var(--kind-graphic)",
};

/**
 * Work out a kind from a URI, the way the server does. Duplicated here on
 * purpose and only for the picker's first guess: the server decides, and
 * anything this gets wrong is corrected the moment the source reports back.
 */
export function kindOfUri(uri) {
  const u = String(uri || "").trim();
  if (!u) return "file";
  if (u.startsWith("exec:")) return "exec";
  if (u.startsWith("web+") || /^https?:\/\//i.test(u)) {
    if (/\.(m3u8|mpd)(\?|$)/i.test(u)) return "stream";
    return "page";
  }
  if (/^(rtmp|rtmps|srt|rtsp|udp|tcp|rist):\/\//i.test(u)) return "stream";
  if (/^file:\/\//i.test(u) || u.startsWith("/") || /^[a-z]:\\/i.test(u)) return "file";
  return "file";
}

/** Group the kinds for the picker, keeping the order the table declares. */
export function grouped(kinds) {
  const out = new Map();
  for (const k of kinds) {
    if (!out.has(k.group)) out.set(k.group, []);
    out.get(k.group).push(k);
  }
  return out;
}

/**
 * The catalogue for the picker. `core.api` `kinds` says what this build has;
 * the table above says how each one looks, matched by `provides`.
 */
export async function loadKinds(client, what) {
  const builtIn = what === "output" ? OUTPUT_KINDS : SOURCE_KINDS;
  try {
    const api = await client.call("core.api", {});
    const kinds = extractKinds(api, what, builtIn);
    if (kinds.length) return kinds;
  } catch {
    /* no core.api on this mixer */
  }
  try {
    const list = await client.call("plugin.list", {});
    const kinds = [];
    for (const plugin of list.plugins || []) {
      for (const provide of plugin.provides || []) {
        if (provide.kind !== what) continue;
        kinds.push({
          id: provide.id,
          title: provide.title || provide.id,
          group: provide.group || "Everything else",
          icon: provide.icon || (what === "output" ? "output" : "stream"),
          description: provide.description || "",
          plugin: plugin.name,
          schema: provide.schema || { type: "object", properties: {} },
          build: (v) => Object.assign({ kind: provide.id }, v),
        });
      }
    }
    if (kinds.length) return builtIn.concat(kinds);
  } catch {
    /* no plugin.list either */
  }
  return builtIn;
}

/**
 * Keep the tiles this build can actually make, in table order.
 *
 * The core's `kinds` is a listing, not a form: it says a kind exists and what
 * it claims, not what to ask an operator for. So it is used to drop tiles this
 * build cannot serve, and the table still draws the ones that are left. A kind
 * with no tile (`test/source`) is not offered yet; it needs its own fields,
 * not a generic address box.
 */
function extractKinds(api, what, builtIn) {
  const reported = (api && api.kinds && api.kinds[what]) || [];
  if (!reported.length) return [];
  const have = new Set(reported.map((k) => k.id));
  return builtIn.filter((t) => !t.provides || t.provides.some((id) => have.has(id)));
}
