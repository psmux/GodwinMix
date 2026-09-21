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
  screen: "M3 5h18v11H3V5zm6 15h6m-3-4v4",
  mic: "M12 3a3 3 0 013 3v5a3 3 0 01-6 0V6a3 3 0 013-3zM6 11a6 6 0 0012 0M12 17v4",
  pattern: "M4 4h16v16H4V4zm5.3 0v16m5.4-16v16M4 9.3h16M4 14.7h16",
  more: "M5 12h.01M12 12h.01M19 12h.01",
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
  {
    id: "test",
    provides: ["test/source"],
    title: "Test pattern",
    group: "Test patterns",
    icon: "pattern",
    description: "Bars and a tone out of the mixer itself. Nothing to plug in and nothing to type.",
    plugin: "built in",
    schema: {
      type: "object",
      required: ["uri"],
      properties: {
        uri: { type: "string", title: "Pattern", examples: ["test://smpte"], description: "test:// and the name of a videotestsrc pattern." },
        name: { type: "string", title: "Name" },
      },
    },
    build: (v) => ({ uri: v.uri, name: v.name || null, kind: null, superimpose: null }),
  },
];

/**
 * The patterns worth a row of their own.
 *
 * `test/source` takes any name `videotestsrc` knows, which is dozens. These
 * four are the ones a setup actually uses: bars to line a screen up, the ball
 * to see whether motion is smooth, black to check a fade, snow to prove a
 * source is live when a still picture would not.
 */
export const TEST_PATTERNS = [
  { uri: "test://smpte", name: "Colour bars", note: "SMPTE bars and a 1 kHz tone" },
  { uri: "test://ball", name: "Moving ball", note: "Motion, for checking frame rate" },
  { uri: "test://black", name: "Black", note: "A flat black frame" },
  { uri: "test://snow", name: "Snow", note: "Noise, which never looks frozen" },
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
  screen: "var(--kind-camera)",
  mic: "var(--kind-graphic)",
  pattern: "var(--kind-other)",
  media: "var(--kind-file)",
  more: "var(--kind-other)",
};

/**
 * The picker's left hand rail: what an operator is looking for, in the order
 * they look for it.
 *
 * The order is deliberate and it is not the order the protocol lists kinds in.
 * Somebody opening this wants the camera that is already plugged in; the
 * address boxes are what they reach for when none of that worked. Wirecast and
 * vMix both put hardware first for the same reason.
 *
 * `provides` are the type ids that land in the category, which is how a
 * plugin's kind finds its place without anything here naming the plugin. A
 * kind that matches nothing falls to More rather than being dropped.
 *
 * `plugin` names the first party plugin a category needs. The category is
 * drawn whether or not it is installed: a camera tab that is missing until
 * somebody runs a terminal command is how this got hard in the first place.
 */
export const CATEGORIES = [
  {
    id: "cameras",
    title: "Cameras",
    icon: "camera",
    devices: true,
    kinds: [],
    provides: ["camera/source"],
    plugin: {
      name: "camera",
      label: "Install camera support",
      line: "Cameras need the camera plugin. It installs into this mixer while it runs, and nothing goes off air.",
    },
    nothing: "No camera answered. Check it is plugged in and that nothing else has it open, then rescan.",
  },
  {
    id: "screens",
    title: "Screens and windows",
    icon: "screen",
    devices: true,
    kinds: [],
    provides: ["screen/source"],
    plugin: {
      name: "screen",
      label: "Install screen capture",
      line: "Capturing a screen needs the screen plugin. It installs into this mixer while it runs.",
    },
    nothing: "Nothing to capture was offered. Rescan after granting this machine's screen recording permission.",
  },
  {
    id: "audio",
    title: "Microphones and audio",
    icon: "mic",
    devices: true,
    kinds: [],
    provides: ["audio-device/source"],
    plugin: {
      name: "audio-device",
      label: "Install audio input support",
      line: "Microphones and sound cards need the audio-device plugin. It installs into this mixer while it runs.",
    },
    nothing: "No sound input answered. Check the machine can hear it, then rescan.",
  },
  {
    id: "files",
    title: "Video and images",
    icon: "file",
    media: true,
    kinds: ["file"],
    provides: ["file/source"],
  },
  { id: "web", title: "Web pages", icon: "page", kinds: ["page"], provides: ["browser/source", "layered/source"] },
  {
    id: "streams",
    title: "Streams and feeds",
    icon: "stream",
    kinds: ["stream"],
    provides: ["rtmp/source", "hls/source", "srt/source", "ndi/source", "ingest/source", "whip/source"],
  },
  { id: "test", title: "Test patterns", icon: "pattern", patterns: true, kinds: ["test"], provides: ["test/source"] },
  { id: "more", title: "More", icon: "more", kinds: ["exec"], provides: ["exec/source"] },
];

/** The category a type id belongs in, for a candidate or a plugin's kind. */
export function categoryOfProvide(id) {
  const want = String(id || "");
  for (const cat of CATEGORIES) {
    if ((cat.provides || []).includes(want)) return cat.id;
  }
  return "more";
}

/** The category one picker kind belongs in. */
export function categoryOf(kind) {
  for (const cat of CATEGORIES) {
    if ((cat.kinds || []).includes(kind.id)) return cat.id;
    if ((kind.provides || []).some((id) => (cat.provides || []).includes(id))) return cat.id;
  }
  return "more";
}

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
 *
 * Both sources are read and both are kept. This used to answer with the built
 * in kinds the moment `core.api` named one, and `core.api` always names one,
 * so every kind a plugin contributed was unreachable from the picker: a
 * camera plugin could be installed and running and the page would still offer
 * four address boxes. The core's list covers what is compiled in and
 * `plugin.list` covers the rest.
 */
export async function loadKinds(client, what, plugins) {
  const builtIn = what === "output" ? OUTPUT_KINDS : SOURCE_KINDS;
  const api = await client.call("core.api", {}).catch(() => null);
  const reported = extractKinds(api, what, builtIn);
  const core = reported.length ? reported : builtIn;
  const installed = plugins || (await listPlugins(client));
  return core.concat(pluginKinds(client, installed, what, core));
}

/** Every plugin this mixer has, or nothing on a core that does not say. */
export async function listPlugins(client) {
  const list = await client.call("plugin.list", {}).catch(() => null);
  return (list && list.plugins) || [];
}

/** Whether a named plugin is installed and loaded, given that listing. */
export function hasPlugin(plugins, name) {
  return (plugins || []).some((p) => p.name === name && p.enabled !== false && !p.problem);
}

/**
 * Keep the tiles this build can actually make, in table order.
 *
 * The core's `kinds` is a listing, not a form: it says a kind exists and what
 * it claims, not what to ask an operator for. So it is used to drop tiles this
 * build cannot serve, and the table still draws the ones that are left.
 */
function extractKinds(api, what, builtIn) {
  const reported = (api && api.kinds && api.kinds[what]) || [];
  if (!reported.length) return [];
  const have = new Set(reported.map((k) => k.id));
  return builtIn.filter((t) => !t.provides || t.provides.some((id) => have.has(id)));
}

/**
 * One picker kind per provide a plugin contributes.
 *
 * `plugin.list` answers with type ids as plain strings (`camera/source`), and
 * the kind is the half after the slash, the same convention the built in ids
 * follow. An older core that answered with objects is still read, because the
 * only cost of doing so is the two lines below.
 *
 * The settings schema is not fetched here. It is one `plugin.describe` per
 * plugin and it is wanted only when somebody opens that kind's form, so it is
 * left as a function the form awaits.
 */
function pluginKinds(client, plugins, what, already) {
  const seen = new Set();
  for (const kind of already) for (const id of kind.provides || []) seen.add(id);
  const out = [];
  for (const plugin of plugins) {
    for (const provide of plugin.provides || []) {
      const entry = typeof provide === "string" ? { id: provide } : provide || {};
      let id = String(entry.id || entry.type || "");
      if (id && !id.includes("/")) id = `${plugin.name}/${id}`;
      const half = entry.kind || id.split("/")[1] || "";
      if (!id || half !== what || seen.has(id)) continue;
      seen.add(id);
      out.push({
        id,
        provides: [id],
        title: entry.title || titleFor(plugin.name, id),
        group: entry.group || "Plugins",
        icon: entry.icon || iconFor(id, what),
        description: entry.description || plugin.description || "",
        plugin: plugin.name,
        schema: entry.schema || (() => provideSchema(client, plugin.name, id)),
        build: buildFor(id),
      });
    }
  }
  return out;
}

/** "camera/source" from the camera plugin reads as "Camera". */
function titleFor(pluginName, id) {
  const half = id.split("/")[1] || "";
  const stem = half === "source" || half === "output" ? pluginName : id;
  return stem.replace(/[-_]/g, " ").replace(/^./, (c) => c.toUpperCase());
}

function iconFor(id, what) {
  const [name] = id.split("/");
  if (ICONS[name]) return name;
  if (name === "audio-device") return "mic";
  if (name === "ndi" || name === "srt" || name === "ingest" || name === "whip") return "stream";
  return what === "output" ? "output" : "device";
}

/**
 * What a plugin's kind sends to `source.add`.
 *
 * `type` names the kind outright and rides underneath the fields the core
 * knows. `uri` is still required and a kind named outright has no address, so
 * the type goes there too, which is what `gmx ctl source add --type` has
 * always done and what the id is then derived from.
 */
function buildFor(id) {
  return (values) => {
    const rest = Object.assign({}, values);
    const name = rest.name;
    delete rest.name;
    const uri = String(rest.uri || id).trim();
    delete rest.uri;
    return Object.assign({ type: id, uri, name: name || null }, rest);
  };
}

/** A provide's settings schema, with a Name field the core always takes. */
async function provideSchema(client, pluginName, id) {
  let found = null;
  try {
    const described = await client.call("plugin.describe", { id: pluginName });
    found = (described && described.schemas && described.schemas[id]) || null;
  } catch {
    /* an older core, or a plugin that went away between two calls */
  }
  const schema = found && typeof found === "object" ? JSON.parse(JSON.stringify(found)) : {};
  schema.type = "object";
  schema.properties = schema.properties || {};
  if (!schema.properties.name) schema.properties.name = { type: "string", title: "Name" };
  return schema;
}

/**
 * The settings schema of a source that already exists, or null.
 *
 * Found by the type the core publishes for it, through the plugin that
 * provides that type. The address cannot stand in: a camera's is cut down to
 * an ellipsis, and its settings used to open as the form of whatever kind came
 * first in the table, with boxes for a file path.
 */
export async function schemaForSource(client, source) {
  const type = String((source && source.type) || "");
  if (!type) return null;
  const plugins = await listPlugins(client);
  const owner = plugins.find((p) =>
    (p.provides || []).some((entry) => {
      const id = typeof entry === "string" ? entry : (entry && (entry.id || entry.type)) || "";
      return id === type || `${p.name}/${id}` === type;
    })
  );
  if (!owner) return null;
  const schema = await provideSchema(client, owner.name, type);
  return Object.keys(schema.properties || {}).length > 1 ? schema : null;
}

// ------------------------------------------------------------- devices

/**
 * What every device plugin can see right now.
 *
 * One call, however many plugins answer it, and the core caps the wait at four
 * and a half seconds. The picker draws its categories before this is asked and
 * fills the rows in when it answers, because a modal that waits on hardware is
 * a modal that looks broken on a machine with a slow camera.
 */
export async function discoverDevices(client, timeoutMs) {
  const answer = await client.call("device.discover", { timeout_ms: timeoutMs || 2000 });
  return (answer && answer.candidates) || [];
}

/**
 * A kind's schema with the box that picks a device turned into a choice of
 * the devices this machine has: a camera, a microphone, a monitor.
 *
 * The box is titled "Camera" and sits at the top of the form, and what it
 * wants is an id, a name exactly as the operating system spells it, or a
 * number. Somebody who typed a name of their own there was refused by the
 * plugin after the form had gone. What `device.discover` found is what can be
 * chosen, by name, with the first one found as the empty choice it always
 * was. Nothing found, or nothing of this kind, leaves the box as it is, so a
 * device the monitor cannot see can still be typed in.
 */
export function withDeviceChoices(schema, kindId, candidates, current) {
  const props = schema && schema.properties;
  if (!props) return schema;
  const mine = (candidates || []).filter((c) => (c.type || c.kind) === kindId && c.params);
  if (!mine.length) return schema;
  // Whatever a candidate carries that the form also asks for is the thing that
  // picks the device: `device` for a camera or a microphone, `monitor` for a
  // screen. Not the label, which is a name for the person and not a choice.
  const keys = Object.keys(props).filter(
    (key) => key !== "label" && key !== "name" && !Array.isArray(props[key].enum) && mine.some((c) => c.params[key] !== undefined && c.params[key] !== "")
  );
  if (!keys.length) return schema;
  const next = Object.assign({}, props);
  for (const key of keys) {
    const having = mine.filter((c) => c.params[key] !== undefined && c.params[key] !== "");
    const text = (props[key].type || "string") === "string";
    // A text setting has always taken empty to mean the first one found, so
    // that stays on offer. A number has no empty, and its first is its 0.
    const values = (text ? [""] : []).concat(having.map((c) => c.params[key]));
    const labels = (text ? ["The first one found"] : []).concat(having.map((c) => c.name || String(c.params[key])));
    const was = current && typeof current === "object" ? current[key] : key === "device" ? current : undefined;
    if (was !== undefined && was !== "" && !values.includes(was)) {
      values.push(was);
      labels.push(String(was));
    }
    next[key] = Object.assign({}, props[key], { enum: values, "x-gmx-labels": labels });
  }
  return Object.assign({}, schema, { properties: next });
}

/** The size a candidate advertises, when it advertises one. */
export function candidateSize(candidate) {
  const params = (candidate && candidate.params) || {};
  const pair = params.best_size || params.size || candidate.best_size;
  if (Array.isArray(pair) && pair.length === 2) return `${pair[0]} x ${pair[1]}`;
  if (typeof pair === "string" && pair.trim()) return pair.trim();
  const w = params.width || candidate.width;
  const h = params.height || candidate.height;
  return w && h ? `${w} x ${h}` : "";
}

/** The `source.add` params a candidate is already carrying. */
export function addRequestFor(candidate) {
  const rest = Object.assign({}, (candidate && candidate.params) || {});
  const type = String((candidate && (candidate.type || candidate.kind)) || "");
  const uri = String(rest.uri || type).trim();
  delete rest.uri;
  delete rest.name;
  return Object.assign({ type, uri, name: (candidate && candidate.name) || null }, rest);
}

/**
 * Whether a source's published address is the address we would add.
 *
 * The core cuts everything after the host off an address before it publishes
 * it, because that is where a stream key lives, so `test://smpte` comes back
 * as `test://smpte/…` and an exact comparison never matches: the picker added
 * a second colour bars every time it was asked for the one the mixer had. An
 * address that is only a scheme and a host loses nothing to the cut, so the
 * cut form identifies it. One with a path does not, and two files would both
 * read `file:///…`, so those only ever match exactly.
 */
export function sameAddress(published, wanted) {
  const have = String(published || "").trim().toLowerCase();
  const want = String(wanted || "").trim().toLowerCase();
  if (!have || !want) return false;
  if (have === want) return true;
  const bare = /^([a-z][a-z0-9+.-]*):\/\/([^/@]+)\/?$/.exec(want);
  return !!bare && have === `${bare[1]}://${bare[2]}/…`;
}

/**
 * Whether this mixer already has the thing a candidate offers.
 *
 * A source record carries an id, a name and a URI and nothing else, and every
 * camera on a machine shares the one URI, so the name is what tells two of
 * them apart. It is the device's own name, because that is what the picker
 * adds it under.
 */
export function alreadyAdded(sources, candidate) {
  const want = String((candidate && candidate.name) || "").trim().toLowerCase();
  const uri = String(addRequestFor(candidate).uri || "").toLowerCase();
  const typed = String((candidate && (candidate.type || candidate.kind)) || "").toLowerCase();
  return (sources || []).some((s) => {
    const name = String(s.name || "").trim().toLowerCase();
    if (want && name === want) return true;
    // A URI that is only the type id names the kind, not this device, so it
    // proves nothing on its own.
    return !!uri && uri !== typed && sameAddress(s.uri, uri);
  });
}

/**
 * What to pass `plugin.add` for a first party plugin.
 *
 * The bare name is the form to prefer, and a marketplace answers with exactly
 * what to send. A mixer that knows no marketplace falls back to the
 * repository the first party plugins live in.
 */
export async function pluginSourceFor(client, name) {
  try {
    const found = await client.call("plugin.search", { term: name });
    const hit = (found.results || []).find((r) => r.name === name);
    if (hit && hit.source) return hit.source;
  } catch {
    /* no marketplace configured, or no network to reach one */
  }
  return "psmux/godwinmix";
}
