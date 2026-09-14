// The connection to the mixer, with no Stream Deck in it.
//
// Everything this plugin does to a mixer happens here, and nothing here knows
// what a Stream Deck action is. That is what makes the behaviour testable
// against the fake core in `test/fake-core.ts` without installing Elgato's
// SDK, which is a package this repository cannot fetch.
//
// It is the same file as the Companion module's `src/link.ts`, deliberately
// copied rather than shared: each integration is a package somebody can lift
// out of this repository whole, and a shared module between two host specific
// plugins would have to be published before either of them could be.
//
// One connection per instance. The client reconnects on its own, keeps a store
// that is snapshot-then-deltas, and fires `flush` at the end of every batch;
// this wraps it in the two things a control surface wants: a callback when
// anything a button looks at has changed, and a small read only view of the
// state that the actions, feedbacks and variables all read.

import { Client, type State } from "./gmx.ts";

export interface LinkOptions {
  /** `http://mixer.local:8080`, as an operator would type it. */
  base: string;
  token?: string | null;
  /** Node 20 has no global WebSocket. The Stream Deck SDK runs on 20. */
  webSocket?: ConnectOptionsWebSocket;
  /** Called whenever something a button looks at has changed. */
  onChange?: (view: View) => void;
  /** Called when the connection opens or closes. */
  onStatus?: (up: boolean, detail: string) => void;
}

type ConnectOptionsWebSocket = ConstructorParameters<typeof Client>[0] extends infer O
  ? O extends { webSocket?: infer W }
    ? W
    : never
  : never;

/** What a source's lamp says. */
export type Tally = "program" | "preview" | "off";

/** Everything a button on a panel can look at, in one flat object. */
export interface View {
  connected: boolean;
  /** The source on air, or null for the slate. */
  program: string | null;
  /** Source id to tally. */
  tally: Record<string, Tally>;
  /** Output id to state: connecting, live, reconnecting or failed. */
  outputs: Record<string, string>;
  /** Source ids, in the order the mixer lists them. */
  sources: Array<{ id: string; name: string; state: string }>;
  uptimeSecs: number;
  /** Programme running time in milliseconds. */
  runningTimeMs: number;
}

export function emptyView(): View {
  return {
    connected: false,
    program: null,
    tally: {},
    outputs: {},
    sources: [],
    uptimeSecs: 0,
    runningTimeMs: 0,
  };
}

/** Read the client's store into the flat view a panel wants. */
export function viewOf(state: State, connected: boolean): View {
  const tally: Record<string, Tally> = {};
  for (const [id, value] of Object.entries(state.tally ?? {})) {
    tally[id] = normaliseTally(value);
  }
  const sources = (state.sources ?? []).map((source) => ({
    id: String(source.id ?? ""),
    name: String(source.name ?? source.id ?? ""),
    state: String(source.state ?? "connecting"),
  }));
  // A mixer that is not sending the derived tally still has a programme, and a
  // tally button must light from it rather than staying dark.
  for (const source of sources) {
    if (!(source.id in tally)) {
      tally[source.id] = state.program === source.id ? "program" : "off";
    }
  }
  const outputs: Record<string, string> = {};
  for (const output of state.outputs ?? []) {
    outputs[String(output.id ?? "")] = String(output.state ?? "connecting");
  }
  return {
    connected,
    program: (state.program as string | null) ?? null,
    tally,
    outputs,
    sources,
    uptimeSecs: Number(state.uptime_secs ?? 0),
    runningTimeMs: Number(state.running_time_ms ?? 0),
  };
}

function normaliseTally(value: unknown): Tally {
  return value === "program" || value === "preview" ? value : "off";
}

/**
 * One mixer, as a control surface sees it.
 *
 * `open()` connects and subscribes; `view` is always readable, whether or not
 * the connection is up. Every method that changes the mixer is one RPC call,
 * the same call the web UI makes.
 */
export class Link {
  private client: Client | null = null;
  private options: LinkOptions;
  private current: View = emptyView();
  private closed = false;

  constructor(options: LinkOptions) {
    this.options = options;
  }

  get view(): View {
    return this.current;
  }

  async open(timeoutMs = 10_000): Promise<void> {
    this.closed = false;
    const client = new Client({
      base: this.options.base,
      token: this.options.token ?? null,
      webSocket: this.options.webSocket,
      // `tally` is the derived document the core will not compute unless a
      // client asks. `program.*` has to be in the list as well: the core sends
      // the tally on the back of `event/program.took`, and a client that did
      // not subscribe to that one never sees a tally change.
      events: ["snapshot", "program.*", "source.*", "output.*", "tally", "resync", "flush"],
      // Subscribed by hand below, so the first subscribe already carries the
      // ext. Letting the client do it and then asking for tally afterwards
      // costs a second round trip on every connect and every reconnect.
      autoSubscribe: false,
    });
    this.client = client;
    client.want("tally", true);

    client.on("open", () => {
      // Every open, including a reconnect: the core forgets what a socket
      // asked for when the socket goes.
      void client.resubscribe(true);
      this.status(true, "connected");
    });
    client.on("close", () => this.status(false, "the mixer closed the connection"));
    client.onFlush((state: State) => {
      this.current = viewOf(state, true);
      this.options.onChange?.(this.current);
    });

    const opened = new Promise<void>((resolve, reject) => {
      const timer = setTimeout(() => {
        off();
        reject(
          new Error(
            `no answer from ${this.options.base}. Is the mixer running, and is the token right?`,
          ),
        );
      }, timeoutMs);
      const off = client.on("open", () => {
        clearTimeout(timer);
        off();
        resolve();
      });
    });
    client.open();
    await opened;
  }

  private status(up: boolean, detail: string): void {
    if (this.closed) return;
    this.current = { ...this.current, connected: up };
    this.options.onStatus?.(up, detail);
    this.options.onChange?.(this.current);
  }

  close(): void {
    this.closed = true;
    this.client?.close();
    this.client = null;
    this.current = { ...emptyView() };
  }

  /** Every call below is the same one the web UI makes. */
  private call(method: string, params: Record<string, unknown> = {}): Promise<unknown> {
    if (!this.client) {
      return Promise.reject(new Error("not connected to a mixer yet"));
    }
    return this.client.callAny(method, params);
  }

  take(source: string | null): Promise<unknown> {
    return this.call("program.take", { source });
  }

  takeScene(scene: string): Promise<unknown> {
    return this.call("program.take", { scene });
  }

  revert(): Promise<unknown> {
    return this.call("program.revert", {});
  }

  addSource(uri: string, id?: string, name?: string): Promise<unknown> {
    const params: Record<string, unknown> = { uri };
    if (id) params.id = id;
    if (name) params.name = name;
    return this.call("source.add", params);
  }

  /**
   * Start an output. There is no `output.start` in the protocol: adding a
   * destination is what starting one means, and the encoder is already
   * running, so it costs nothing on air.
   */
  startOutput(uri: string, id?: string): Promise<unknown> {
    const params: Record<string, unknown> = { uri };
    if (id) params.id = id;
    return this.call("output.add", params);
  }

  stopOutput(id: string): Promise<unknown> {
    return this.call("output.remove", { id });
  }

  reconnectOutput(id: string): Promise<unknown> {
    return this.call("output.reconnect", { id });
  }

  setAudio(id: string, gain?: number, muted?: boolean): Promise<unknown> {
    const params: Record<string, unknown> = { id };
    if (gain !== undefined) params.gain = gain;
    if (muted !== undefined) params.muted = muted;
    return this.call("source.audio.set", params);
  }
}

/** Seconds as `1:02:03`, which is what an uptime variable should read like. */
export function clock(totalSeconds: number): string {
  const seconds = Math.max(0, Math.floor(totalSeconds));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  const rest = seconds % 60;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${hours}:${pad(minutes)}:${pad(rest)}`;
}
