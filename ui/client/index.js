// @godwinmix/client, for the browser: connect, subscribe, a typed state store,
// typed calls, and the frame decoders. A panel is handed one of these and never
// touches fetch, WebSocket or JSON itself.
//
// The `ext` bookkeeping here is what the core cares about. A live tile asks for
// pictures; the client adds up what every asker wants, subscribes once at the
// widest width and highest rate anyone needs, and unsubscribes the moment the
// last asker lets go. A gallery on icon mode costs the core nothing at all.

import { Store } from "./store.js";
import { RpcTransport } from "./transport-rpc.js";
import { asRpcError } from "./errors.js";
import { SheetPainter, sheetWidthFor } from "./frames.js";

export { RpcError, CODES } from "./errors.js";
export { SheetPainter, sheetWidthFor, parseFrame, HEADER_BYTES } from "./frames.js";
export { Store, emptyState } from "./store.js";

const TOKEN_KEY = "gmx.token";

/** The events a UI wants. Anything not listed is never sent to us. */
const WANTED_EVENTS = [
  "snapshot",
  "program.*",
  "preview.*",
  "source.*",
  "output.*",
  "scene.*",
  "media.*",
  "adbreak.*",
  "plugin.*",
  "multiview.*",
  "meters",
  "tally",
  "alert",
  "resync",
  "flush",
];

export class Client {
  constructor(transport, store) {
    this.transport = transport;
    this.store = store;
    this.sheet = new SheetPainter();
    this.listeners = new Map();
    this._ext = new Map(); // key -> Map(holder id -> request)
    this._holder = 0;
    this._subscribed = null;
    this._resubTimer = null;
    this._flushTimer = null;
  }

  get state() {
    return this.store.state;
  }

  get legacy() {
    return this.transport.name === "legacy";
  }

  /** What this core can actually do, so a panel degrades instead of failing. */
  get capabilities() {
    return this.transport.capabilities || {
      subscriptionIsReal: true,
      framesHaveHeader: true,
      rename: true,
      colour: true,
      scenes: true,
      palette: true,
    };
  }

  // ---------------------------------------------------------------- events

  /** on("flush" | "event" | "open" | "close" | "resync", fn) -> off */
  on(name, fn) {
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name).add(fn);
    return () => this.listeners.get(name).delete(fn);
  }

  emit(name, arg) {
    const set = this.listeners.get(name);
    if (!set) return;
    for (const fn of set) {
      try {
        fn(arg);
      } catch (e) {
        console.error(`listener for ${name} threw`, e);
      }
    }
  }

  /**
   * Render at flush. Returns an unsubscribe function.
   *
   * The callback is also run once, straight away, with whatever state is
   * already known. A panel mounted mid session (a plugin's, or one the
   * operator just added to a slot) would otherwise sit blank until the next
   * event, which on a quiet mixer can be a long time.
   */
  onRender(fn) {
    const off = this.store.subscribe(fn);
    try {
      fn(this.store.state);
    } catch (e) {
      console.error("panel threw on its first render", e);
    }
    return off;
  }

  // ---------------------------------------------------------------- calls

  /** Every method goes through here, so every failure has the one error shape. */
  async call(method, params) {
    try {
      return await this.transport.call(method, params || {});
    } catch (e) {
      const err = asRpcError(e);
      this.emit("error", { method, error: err });
      throw err;
    }
  }

  upload(name, file, onProgress) {
    return this.transport.upload(name, file, onProgress);
  }

  snapshotUrl(name, width) {
    return this.transport.snapshotUrl(name, width);
  }

  // ---------------------------------------------------------------- ext

  /**
   * Ask for an expensive stream. Returns a handle; call `release()` when the
   * thing that wanted it goes away (a tile scrolled out, the gallery stepped
   * down to icons, the panel was removed).
   *
   *   const want = client.want("multiview", {fps: 8, width: 960});
   *   want.update({width: 1280});
   *   want.release();
   */
  want(key, value) {
    const id = ++this._holder;
    if (!this._ext.has(key)) this._ext.set(key, new Map());
    this._ext.get(key).set(id, value === undefined ? true : value);
    this._scheduleResub();
    const self = this;
    return {
      update(next) {
        const bag = self._ext.get(key);
        if (!bag || !bag.has(id)) return;
        bag.set(id, next === undefined ? true : next);
        self._scheduleResub();
      },
      release() {
        const bag = self._ext.get(key);
        if (!bag) return;
        bag.delete(id);
        self._scheduleResub();
      },
    };
  }

  /** The union of everything asked for: the widest width, the highest rate. */
  extSpec() {
    const ext = {};
    for (const [key, bag] of this._ext) {
      if (bag.size === 0) continue;
      let merged = null;
      for (const value of bag.values()) {
        if (value === true) {
          merged = merged === null ? true : merged;
          continue;
        }
        if (merged === null || merged === true) merged = {};
        for (const [k, v] of Object.entries(value)) {
          merged[k] = typeof v === "number" ? Math.max(merged[k] ?? 0, v) : v;
        }
      }
      ext[key] = merged === null ? true : merged;
    }
    return ext;
  }

  _scheduleResub() {
    // A gallery switching mode touches every tile. Coalesce so the core gets
    // one subscribe rather than one per tile.
    if (this._resubTimer) return;
    this._resubTimer = setTimeout(() => {
      this._resubTimer = null;
      this._resubscribe();
    }, 30);
  }

  async _resubscribe() {
    const spec = { events: WANTED_EVENTS, ext: this.extSpec() };
    const same = JSON.stringify(spec) === JSON.stringify(this._subscribed);
    if (same) return;
    this._subscribed = spec;
    try {
      await this.transport.subscribe(spec);
    } catch (e) {
      console.warn("core.subscribe was refused", e);
    }
  }

  /** Force a fresh subscribe, used on reconnect and on resync. */
  resubscribe() {
    this._subscribed = null;
    return this._resubscribe();
  }

  // ---------------------------------------------------------------- wiring

  handleEvent(name, params) {
    const s = this.store;
    switch (name) {
      case "snapshot":
        s.snapshot(params.state, params.seq);
        if (params.state.multiview) this.sheet.setLayout(params.state.multiview);
        break;
      case "flush":
        s.patch({ seq: params.seq ?? s.state.seq });
        s.flush();
        break;
      case "resync":
        this.emit("resync", params);
        this.resubscribe();
        this.call("core.info").catch(() => {});
        break;
      case "program.took":
        // The two fields the status document has, kept apart here as well.
        // Folding a scene name into `program` made the live state disagree
        // with every snapshot, so a reconnect under a multi item scene left
        // the header reading "black" while the mixer was on air.
        s.patch({
          program: params.source ?? null,
          scene: params.scene ?? null,
          tookAt: Date.now(),
        });
        break;
      case "preview.changed":
        s.patch({ preview: params.scene ?? null });
        break;
      case "source.state":
        s.patchSource(params.source, { state: params.state, detail: params.detail });
        break;
      case "source.position":
        this.emit("position", params);
        break;
      case "output.state":
        s.patchOutput(params.output, { state: params.state, reconnects: params.reconnects });
        break;
      case "adbreak.changed":
        s.patch({ ad: params.ad ?? null });
        break;
      case "media.changed":
        this.emit("media-changed", params);
        break;
      case "meters":
        s.setMeters(params.program, params.sources);
        this.emit("meters", params);
        break;
      case "tally":
        s.patch({ tally: params.sources || {} });
        break;
      case "multiview.layout":
        this.sheet.setLayout(params);
        s.patch({ layout: params });
        break;
      case "alert":
        s.addAlert(params);
        this.emit("alert", params);
        break;
      default:
        break;
    }
    this.emit("event", { name, params });
  }

  handleFrame(frame) {
    this.sheet.push(frame);
    this.emit("frame", frame);
  }

  close() {
    this.transport.close();
    this.sheet.destroy();
  }
}

// -------------------------------------------------------------------- token

export function storedToken() {
  try {
    return localStorage.getItem(TOKEN_KEY) || null;
  } catch {
    return null;
  }
}

export function storeToken(token) {
  try {
    if (token) localStorage.setItem(TOKEN_KEY, token);
    else localStorage.removeItem(TOKEN_KEY);
  } catch {
    /* a private window simply forgets between reloads */
  }
}

/**
 * Move the LiveboxMix keys over once. Cheap, runs before anything reads them,
 * and means an operator who upgrades does not have to find their token again.
 */
export function migrateLegacyKeys() {
  const pairs = [["lbx.token", "gmx.token"], ["lbx.adUri", "gmx.adUri"], ["lbx.strips", "gmx.strips"]];
  try {
    for (const [from, to] of pairs) {
      const value = localStorage.getItem(from);
      if (value !== null && localStorage.getItem(to) === null) localStorage.setItem(to, value);
      if (value !== null) localStorage.removeItem(from);
    }
  } catch {
    /* nothing to migrate in a window that has no storage */
  }
}

// -------------------------------------------------------------------- connect

/**
 * Which protocol this server speaks. One request, no socket, no guessing:
 * `/api/v1/core/info` exists only on a core that has `/rpc`.
 *
 * 401 counts as present: the endpoint is there, the token is wrong, and asking
 * for a token is a better next step than falling back to an API that will
 * refuse us in the same way.
 */
export async function detect(base, token) {
  const headers = token ? { Authorization: "Bearer " + token } : {};
  try {
    const res = await fetch(new URL("/api/v1/core/info", base), { headers });
    if (res.ok || res.status === 401) return "rpc";
  } catch {
    /* a network failure means we try the old API and let it report */
  }
  return "legacy";
}

/**
 * Connect and return a ready client.
 *
 * @param {{base?: string, token?: ?string, force?: "rpc"|"legacy"}} opts
 */
export async function connect(opts = {}) {
  const base = opts.base || location.origin;
  const token = opts.token !== undefined ? opts.token : storedToken();
  const kind = opts.force || (await detect(base, token));

  const store = new Store();
  let client;
  const hooks = {
    onOpen: () => {
      store.patch({ connected: true });
      store.flush(true);
      client.emit("open");
      client.resubscribe();
    },
    onClose: () => {
      store.patch({ connected: false });
      store.flush(true);
      client.emit("close");
    },
    onEvent: (name, params) => client.handleEvent(name, params),
    onFrame: (frame) => client.handleFrame(frame),
  };

  // The adapter for a core with no `/rpc` is eleven kilobytes that a current
  // core never needs, so it is fetched only when `detect` says this one is old.
  // Nothing runs unless asked (01, principle 2), and nothing is downloaded
  // unless asked either.
  const transport =
    kind === "rpc"
      ? new RpcTransport({ base, token, hooks })
      : new (await import("./transport-legacy.js")).LegacyTransport({ base, token, hooks });
  client = new Client(transport, store);
  transport.open();
  return client;
}
