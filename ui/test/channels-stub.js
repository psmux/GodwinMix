// A stand in for a core with channels, answering in the shapes the real
// channel.* methods answer with (checked against a running core and the
// ingest plugin), for the tests and the preview page. It keeps
// channels in memory, answers the channel.* methods and sends
// event/channel.changed the way the core will, through the client's own
// `event` listeners.

import { slugify } from "../panels/channels/model.js";

export class ChannelStub {
  constructor(opts = {}) {
    this.channels = new Map();
    this.secrets = new Map();
    this.calls = [];
    this.listeners = new Map();
    this.patterns = new Map();
    this.state = { connected: true, sources: [], outputs: [] };
    this.urls = opts.urls || ["rtmp://10.0.0.5:1935", "rtmp://192.168.1.20:1935"];
    this.n = 0;
  }

  on(name, fn) {
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name).add(fn);
    return () => this.listeners.get(name).delete(fn);
  }

  emit(name, arg) {
    for (const fn of this.listeners.get(name) || []) fn(arg);
  }

  /** The client's subscription bookkeeping, counted so a test can see it. */
  listen(pattern) {
    this.patterns.set(pattern, (this.patterns.get(pattern) || 0) + 1);
    return () => {
      const n = this.patterns.get(pattern) - 1;
      if (n > 0) this.patterns.set(pattern, n);
      else this.patterns.delete(pattern);
    };
  }

  onRender() {
    return () => {};
  }

  /** Send a channel as the core would after any change to it. */
  changed(id) {
    this.emit("event", { name: "channel.changed", params: { channel: this.channels.get(id) } });
  }

  async call(method, params = {}) {
    this.calls.push({ method, params });
    const fn = METHODS[method];
    if (!fn) throw Object.assign(new Error(`no method ${method}`), { code: -32601, data: {} });
    return structuredClone(fn.call(this, params));
  }

  channel(id) {
    const c = this.channels.get(id);
    if (!c) throw Object.assign(new Error(`There is no channel "${id}". Add it first.`), { code: -32602, data: { id } });
    return c;
  }

  newKey(c, label) {
    this.n += 1;
    // Different every time, and the same on every run, so a test can say what it expects.
    const abc = "Xq7Lm2Pz9Rv4Tw8YbN3cK6hJ5dF";
    const secret = "k" + this.n + [...Array(14)].map((_, i) => abc[(i * 7 + this.n * 5) % abc.length]).join("");
    const key = { id: `key-${this.n}`, label: label || `Key ${c.keys.length + 1}`, created: "2026-09-29T10:00:00Z", hint: secret.slice(-4) };
    c.keys.push(key);
    this.secrets.set(key.id, secret);
    return { id: key.id, label: key.label, secret };
  }
}

/** The ports the channels need, as the core reports them, all open. */
function listeners(channels) {
  const on = (p) => channels.filter((c) => c.enabled && (c.protocols || ["rtmp"]).includes(p)).map((c) => c.id);
  const rows = [
    { protocol: "rtmp", transport: "tcp", port: 1935, open: on("rtmp").length > 0, because: on("rtmp") },
    { protocol: "srt", transport: "udp", port: 9000, open: on("srt").length > 0, because: on("srt") },
    { protocol: "whip", transport: "tcp", port: 8080, open: on("whip").length > 0, because: on("whip") },
  ];
  for (const c of channels.filter((c) => c.enabled && c.rtmps && c.rtmps.enabled)) {
    rows.push({ protocol: "rtmps", transport: "tcp", port: c.rtmps.port, open: true, because: [c.id] });
  }
  return rows;
}

const METHODS = {
  "channel.list"() {
    const channels = [...this.channels.values()];
    return { channels, rtmp: { port: 1935, urls: this.urls, listening: true }, listeners: listeners(channels), hosts: ["10.0.0.5", "192.168.1.20"], certificate: this.certificate || null };
  },
  "channel.certificate.generate"() {
    this.certificate = { source: "self_signed", names: ["10.0.0.5", "localhost"], fingerprint: "AB:CD:EF:01:23:45:67:89:AB:CD:EF", created: "2026-09-30T10:00:00Z" };
    return this.certificate;
  },
  "channel.get"({ id }) {
    return this.channel(id);
  },
  "channel.add"({ name, app, auto_source, key_mode }) {
    const id = app || slugify(name);
    if (this.channels.has(id)) throw Object.assign(new Error(`A channel called "${id}" is here already. Pick another name.`), { code: -32602, data: { id } });
    const c = {
      id, name, app: id, enabled: true, auto_source: auto_source !== false, key_mode: key_mode || "query", keys: [],
      protocols: ["rtmp"], rtmps: { enabled: false, port: 443 },
      publish: { server: `${this.urls[0]}/${id}`, example: `${this.urls[0]}/${id}/main?psk=<key>` },
      streams: [], destinations: [],
    };
    this.channels.set(id, c);
    const key = this.newKey(c);
    this.changed(id);
    return { channel: c, key };
  },
  "channel.set"(p) {
    const c = this.channel(p.id);
    for (const k of ["name", "enabled", "auto_source", "key_mode", "protocols", "rtmps"]) if (p[k] !== undefined) c[k] = p[k];
    this.changed(c.id);
    return c;
  },
  "channel.remove"({ id }) {
    this.channel(id);
    this.channels.delete(id);
    this.emit("event", { name: "channel.removed", params: { id } });
    return { removed: id };
  },
  "channel.key.add"({ id, label }) {
    const key = this.newKey(this.channel(id), label);
    this.changed(id);
    return { key };
  },
  "channel.key.reveal"({ id, key }) {
    const c = this.channel(id);
    if (!c.keys.some((k) => k.id === key)) throw Object.assign(new Error(`there is no key '${key}'.`), { code: -32004, data: { channel: id } });
    return { secret: this.secrets.get(key) };
  },
  "channel.key.remove"({ id, key }) {
    const c = this.channel(id);
    c.keys = c.keys.filter((k) => k.id !== key);
    this.changed(id);
    return c;
  },
  "channel.destination.add"(p) {
    const c = this.channel(p.id);
    // The core names a destination after its label, or its platform.
    const base = slugify(p.label || "") || p.platform;
    let n = 1;
    while (c.destinations.some((d) => d.id === (n === 1 ? base : `${base}-${n}`))) n++;
    c.destinations.push({
      id: n === 1 ? base : `${base}-${n}`, platform: p.platform, label: p.label || "", uri_host: (p.server || "").replace(/^(\w+:\/\/[^/?]+).*$/, "$1"),
      // A whole address pasted into a custom server carries its own key.
      has_key: !!p.key || p.platform === "srt" || (p.platform === "custom" && /^\w+:\/\/[^/]+\/[^/]+\/./.test(p.server || "")), stream: p.stream || "*", enabled: p.enabled !== false, state: p.enabled === false ? "off" : "waiting",
      since_ms: 0, kbps: 0, reconnects: 0, error: null,
    });
    this.changed(c.id);
    return c;
  },
  "channel.destination.set"(p) {
    const c = this.channel(p.id);
    const d = c.destinations.find((x) => x.id === p.destination);
    if (p.enabled !== undefined) {
      d.enabled = p.enabled;
      // On again, it waits for the stream; the listener's report moves it on.
      d.state = p.enabled ? "waiting" : "off";
    }
    if (p.key) d.has_key = true;
    if (p.label !== undefined) d.label = p.label;
    if (p.stream !== undefined) d.stream = p.stream;
    this.changed(c.id);
    return c;
  },
  "channel.destination.remove"(p) {
    const c = this.channel(p.id);
    c.destinations = c.destinations.filter((d) => d.id !== p.destination);
    this.changed(c.id);
    return c;
  },
};

/** A publisher arriving on a channel, as the server would describe it. */
export function liveStream(name, over = {}) {
  return {
    name, state: "live", since_ms: Date.now() - 754000, from: "10.0.0.23:51514", key: "key-1", dropped_gops: 0,
    video: { codec: "h264", width: 1920, height: 1080, fps: 30, kbps: 4500 },
    audio: { codec: "aac", channels: 2, sample_rate: 48000, kbps: 160 },
    source: null, ...over,
  };
}
