// The connection: one WebSocket, a store beside it, and the typed methods the
// generator writes.
//
// A surface is handed one of these and never touches fetch, WebSocket or JSON
// itself.

import { asRpcError, RpcError } from "./errors.ts";
import { parseFrame, type Frame } from "./frames.ts";
import { GeneratedMethods } from "./generated/protocol.ts";
import type { EventName, EventPayloads, MethodName, MethodParams, MethodResults } from "./generated/protocol.ts";
import { RpcSocket, type WebSocketFactory } from "./rpc.ts";
import { Store, type Listener, type State } from "./store.ts";
import { mjpegUrl, rpcUrl, snapshotUrl, whepUrl } from "./urls.ts";

/** The events a surface normally wants. Anything not listed is never sent. */
export const UI_EVENTS = [
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

export interface ConnectOptions {
  /** `http://host:8080`, `https://...`, or a ws address. */
  base?: string;
  token?: string | null;
  /** Node 20 has no global WebSocket: pass `(await import("ws")).WebSocket`. */
  webSocket?: WebSocketFactory;
  /** Event patterns. Defaults to [`UI_EVENTS`]. */
  events?: string[];
  /** Subscribe as soon as the socket opens. On by default. */
  autoSubscribe?: boolean;
}

/** A handle on one expensive stream. Release it and the core stops the work. */
export interface Want {
  update(value: unknown): void;
  release(): void;
}

type Handler = (arg: never) => void;

export class Client extends GeneratedMethods {
  socket: RpcSocket;
  store: Store;
  base: string;
  token: string | null;
  events: string[];

  private listeners = new Map<string, Set<Handler>>();
  private ext = new Map<string, Map<number, unknown>>();
  private holder = 0;
  private subscribed: string | null = null;
  private resubTimer: ReturnType<typeof setTimeout> | null = null;

  constructor(opts: ConnectOptions = {}) {
    super();
    this.base = opts.base || (typeof location !== "undefined" ? location.origin : "http://127.0.0.1:8080");
    this.token = opts.token ?? null;
    this.events = opts.events || UI_EVENTS;
    this.store = new Store();
    const autoSubscribe = opts.autoSubscribe !== false;
    this.socket = new RpcSocket(
      rpcUrl(this.base, this.token),
      {
        onOpen: () => {
          this.store.patch({ connected: true });
          this.store.flush(true);
          this.emit("open", undefined);
          if (autoSubscribe) void this.resubscribe();
        },
        onClose: () => {
          this.store.patch({ connected: false });
          this.store.flush(true);
          this.emit("close", undefined);
        },
        onNotify: (method, params) => {
          if (!method.startsWith("event/")) return;
          this.handleEvent(method.slice(6) as EventName, params);
        },
        onBinary: (data) => {
          const frame = parseFrame(data);
          if (frame) this.emit("frame", frame);
        },
      },
      opts.webSocket,
    );
  }

  get state(): State {
    return this.store.state;
  }

  open(): void {
    this.socket.open();
  }

  close(): void {
    this.socket.close();
  }

  // ------------------------------------------------------------------ calls

  /** Every method goes through here, so every failure has the one error shape. */
  override async _call(method: string, params: Record<string, unknown>): Promise<unknown> {
    try {
      return await this.socket.call(method, params);
    } catch (e) {
      const error = asRpcError(e);
      this.emit("error", { method, error });
      throw error;
    }
  }

  /** A typed call by name, for code that holds the method name in a variable. */
  call<M extends MethodName>(method: M, params?: MethodParams[M]): Promise<MethodResults[M]> {
    return this._call(method, (params || {}) as Record<string, unknown>) as Promise<MethodResults[M]>;
  }

  /** A method this api_level has never heard of, such as one a plugin added. */
  callAny(method: string, params?: Record<string, unknown>): Promise<unknown> {
    return this._call(method, params || {});
  }

  // ----------------------------------------------------------------- events

  /** on("flush" | "event" | "open" | "close" | "frame" | "error", fn) -> off */
  on(name: "flush", fn: (state: State) => void): () => void;
  on(name: "frame", fn: (frame: Frame) => void): () => void;
  on(name: "event", fn: (e: { name: EventName; params: unknown }) => void): () => void;
  on(name: "error", fn: (e: { method: string; error: RpcError }) => void): () => void;
  on(name: string, fn: (arg: never) => void): () => void;
  on(name: string, fn: (arg: never) => void): () => void {
    if (!this.listeners.has(name)) this.listeners.set(name, new Set());
    this.listeners.get(name)!.add(fn);
    return () => {
      this.listeners.get(name)?.delete(fn);
    };
  }

  emit(name: string, arg: unknown): void {
    const set = this.listeners.get(name);
    if (!set) return;
    for (const fn of set) {
      try {
        (fn as (a: unknown) => void)(arg);
      } catch (e) {
        console.error(`a listener for ${name} threw`, e);
      }
    }
  }

  /**
   * Render at flush. Returns an unsubscribe function.
   *
   * The callback also runs once straight away with whatever state is known. A
   * surface mounted mid session would otherwise sit blank until the next
   * event, which on a quiet mixer can be a long time.
   */
  onFlush(fn: Listener): () => void {
    const off = this.store.subscribe(fn);
    try {
      fn(this.store.state);
    } catch (e) {
      console.error("a surface threw on its first render", e);
    }
    return off;
  }

  /** Resolves at the next flush. What a script wants after subscribing. */
  settled(timeoutMs = 10000): Promise<State> {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        off();
        reject(new RpcError(-32001, `no event/flush in ${timeoutMs} ms. Is the core subscribed?`));
      }, timeoutMs);
      const off = this.store.subscribe((state) => {
        clearTimeout(timer);
        off();
        resolve(state);
      });
    });
  }

  // -------------------------------------------------------------------- ext

  /**
   * Ask for an expensive stream. Returns a handle; release it when the thing
   * that wanted it goes away.
   *
   *   const want = client.want("multiview", { fps: 8, width: 960 });
   *   want.update({ width: 1280 });
   *   want.release();
   *
   * The client adds up what every asker wants and subscribes once at the widest
   * width and the highest rate anyone needs, so a wall of tiles is one
   * subscription and a surface that asks for nothing costs the core nothing.
   */
  want(key: string, value?: unknown): Want {
    const id = ++this.holder;
    if (!this.ext.has(key)) this.ext.set(key, new Map());
    this.ext.get(key)!.set(id, value === undefined ? true : value);
    this.scheduleResub();
    return {
      update: (next: unknown) => {
        const bag = this.ext.get(key);
        if (!bag || !bag.has(id)) return;
        bag.set(id, next === undefined ? true : next);
        this.scheduleResub();
      },
      release: () => {
        this.ext.get(key)?.delete(id);
        this.scheduleResub();
      },
    };
  }

  /** The union of everything asked for: the widest width, the highest rate. */
  extSpec(): Record<string, unknown> {
    const ext: Record<string, unknown> = {};
    for (const [key, bag] of this.ext) {
      if (bag.size === 0) continue;
      let merged: unknown = null;
      for (const value of bag.values()) {
        if (value === true) {
          merged = merged === null ? true : merged;
          continue;
        }
        if (merged === null || merged === true) merged = {};
        const into = merged as Record<string, unknown>;
        for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
          into[k] = typeof v === "number" ? Math.max((into[k] as number) ?? 0, v) : v;
        }
      }
      ext[key] = merged === null ? true : merged;
    }
    return ext;
  }

  private scheduleResub(): void {
    // A gallery switching mode touches every tile. Coalesce, so the core gets
    // one subscribe rather than one per tile.
    if (this.resubTimer) return;
    this.resubTimer = setTimeout(() => {
      this.resubTimer = null;
      void this.resubscribe();
    }, 30);
  }

  /** Send `core.subscribe` if what we want has changed since last time. */
  async resubscribe(force?: boolean): Promise<void> {
    const spec = { events: this.events, ext: this.extSpec() };
    const key = JSON.stringify(spec);
    if (!force && key === this.subscribed) return;
    this.subscribed = key;
    try {
      const result = await this.coreSubscribe(spec as MethodParams["core.subscribe"]);
      this.emit("subscribed", result);
    } catch (e) {
      this.subscribed = null;
      this.emit("error", { method: "core.subscribe", error: asRpcError(e) });
    }
  }

  // ----------------------------------------------------------------- wiring

  handleEvent<E extends EventName>(name: E, params: unknown): void {
    const s = this.store;
    const p = params as Record<string, never>;
    switch (name) {
      case "snapshot": {
        const snap = params as EventPayloads["snapshot"];
        s.snapshot(snap.state, snap.seq);
        break;
      }
      case "flush": {
        const flush = params as EventPayloads["flush"];
        s.patch({ seq: flush.seq ?? s.state.seq });
        s.flush();
        break;
      }
      case "resync":
        this.emit("resync", params);
        void this.resubscribe(true);
        break;
      case "program.took": {
        const took = params as EventPayloads["program.took"];
        s.patch({ program: took.source ?? took.scene ?? null });
        break;
      }
      case "source.state": {
        const ev = params as EventPayloads["source.state"];
        if (ev.source) s.patchSource(ev.source, { state: ev.state });
        break;
      }
      case "output.state": {
        const ev = params as EventPayloads["output.state"];
        if (ev.output) s.patchOutput(ev.output, { state: ev.state, reconnects: ev.reconnects });
        break;
      }
      case "adbreak.changed":
        s.patch({ ad: (params as EventPayloads["adbreak.changed"]).ad ?? null });
        break;
      case "meters": {
        const m = params as EventPayloads["meters"];
        s.setMeters(m.program, m.sources);
        this.emit("meters", m);
        break;
      }
      case "tally":
        s.patch({ tally: ((params as EventPayloads["tally"]).sources || {}) as Record<string, string> });
        break;
      case "multiview.layout":
        s.patch({ layout: params as EventPayloads["multiview.layout"] });
        break;
      case "alert":
        s.addAlert(params as EventPayloads["alert"]);
        this.emit("alert", params);
        break;
      default:
        // preview.changed and anything a core one level ahead sends. Kept, not
        // dropped: a surface that knows the name can listen for it.
        if ((name as string) === "preview.changed") {
          s.patch({ preview: (p as { scene?: string }).scene ?? null });
        }
        break;
    }
    this.emit("event", { name, params });
  }

  // ------------------------------------------------------------------ video

  /** `GET /api/v1/snapshot/{name}`, token included. */
  snapshotUrl(name: string, width?: number): string {
    return snapshotUrl(this.base, name, { width, token: this.token, cacheBust: true });
  }

  /** `GET /mjpeg/{name}`, the multipart stream. In a browser this is an <img src>. */
  mjpegUrl(name: string, opts: { width?: number; fps?: number } = {}): string {
    return mjpegUrl(this.base, name, { ...opts, token: this.token });
  }

  /** `POST /whep/{name}`, for [`attachWhep`]. */
  whepUrl(name: string): string {
    return whepUrl(this.base, name, this.token);
  }
}

/**
 * Connect and return a ready client.
 *
 * ```ts
 * const client = await connect({ base: "http://127.0.0.1:8080", token });
 * await client.settled();
 * await client.programTake({ source: "cam1" });
 * ```
 */
export async function connect(opts: ConnectOptions & { timeoutMs?: number } = {}): Promise<Client> {
  const client = new Client(opts);
  const timeoutMs = opts.timeoutMs ?? 10000;
  client.open();
  await new Promise<void>((resolve, reject) => {
    const timer = setTimeout(() => {
      off();
      client.close();
      reject(
        new RpcError(-32001, `no answer from ${client.base}. Is the core running, and is the token right?`, {
          retryable: true,
        }),
      );
    }, timeoutMs);
    const off = client.on("open", () => {
      clearTimeout(timer);
      off();
      resolve();
    });
  });
  return client;
}
