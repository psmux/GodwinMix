// The legacy transport: today's REST endpoints and today's broadcast socket,
// dressed as `/rpc`, so the new UI works against a server with no `/rpc` yet.
// Every panel calls `source.audio.set`; only this file knows the wire is
// `POST /api/sources/{id}/audio`. Delete it once `/rpc` ships.
//
// Three things the old server cannot do, and what happens instead:
//   * `core.subscribe` has no effect: frames go to every client regardless. The
//     client still stops decoding them when no tile is live, which is the
//     expensive half, and says so in `capabilities.subscriptionIsReal`.
//   * Frames have no 16 byte header, so they are wrapped with the layout id the
//     status document implies and `frames.js` sees one shape either way.
//   * `source.set {name, color}` does not exist: it answers -32601 and the tray
//     keeps the change on this device, marked as local.

import { RpcError, CODES } from "./errors.js";
import { bareFrame } from "./frames.js";
import { httpUpload } from "./transport-rpc.js";

const POLL_MS = 2000;

/** Legacy event name to the name the rest of the client knows. */
const EVENT_NAMES = {
  took: "program.took",
  source_state_changed: "source.state",
  output_state_changed: "output.state",
  ad_break_changed: "adbreak.changed",
  media_changed: "media.changed",
  source_position: "source.position",
  alert: "alert",
};

export class LegacyTransport {
  constructor(opts) {
    this.name = "legacy";
    this.base = opts.base;
    this.token = opts.token;
    this.hooks = opts.hooks;
    this.ws = null;
    this.closed = false;
    this.poll = null;
    this.layoutId = 0;
    this.wantFrames = true;
    this.seq = 0;
  }

  capabilities = {
    subscriptionIsReal: false,
    framesHaveHeader: false,
    rename: false,
    colour: false,
    scenes: false,
    palette: false,
  };

  open() {
    this.closed = false;
    // The status fetch happens before the socket so that a mixer with a token
    // configured asks for one over HTTP, where a 401 is visible, rather than
    // dropping a socket with no explanation.
    this._status()
      .then(() => this._connect())
      .catch(() => this._connect());
    this.poll = setInterval(() => this._status().catch(() => {}), POLL_MS);
  }

  close() {
    this.closed = true;
    if (this.poll) clearInterval(this.poll);
    this.poll = null;
    if (this.ws) this.ws.close();
    this.ws = null;
  }

  subscribe(spec) {
    // Nothing to send. Remember whether pictures are wanted so the frame arm
    // can drop them before they cost a decode.
    const mv = spec && spec.ext && spec.ext.multiview;
    this.wantFrames = !!mv;
    return Promise.resolve({ ext: { multiview: this.wantFrames } });
  }

  // ---------------------------------------------------------------- socket

  _wsUrl() {
    const u = new URL("/ws", this.base);
    u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
    if (this.token) u.searchParams.set("token", this.token);
    return u.toString();
  }

  _connect() {
    if (this.closed) return;
    let ws;
    try {
      ws = new WebSocket(this._wsUrl());
    } catch {
      setTimeout(() => this._connect(), 1000);
      return;
    }
    ws.binaryType = "arraybuffer";
    this.ws = ws;
    ws.onopen = () => this.hooks.onOpen();
    ws.onclose = () => {
      this.hooks.onClose();
      if (!this.closed) setTimeout(() => this._connect(), 1000);
    };
    ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") {
        if (this.wantFrames) this.hooks.onFrame(bareFrame(ev.data, this.layoutId));
        return;
      }
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch {
        return;
      }
      this._translate(msg);
    };
  }

  _translate(ev) {
    this.seq += 1;
    switch (ev.type) {
      case "status": {
        const { type, ...state } = ev;
        this._noteLayout(state.multiview);
        this.hooks.onEvent("snapshot", { seq: this.seq, state });
        this.hooks.onEvent("multiview.layout", this._layout(state.multiview));
        break;
      }
      case "audio_level":
        this.hooks.onEvent("meters", { program: ev.peak_db });
        break;
      case "source_audio_level":
        this.hooks.onEvent("meters", { sources: { [ev.source]: ev.peak_db } });
        break;
      default: {
        const name = EVENT_NAMES[ev.type];
        if (!name) return;
        const { type, ...params } = ev;
        this.hooks.onEvent(name, params);
      }
    }
    this.hooks.onEvent("flush", { seq: this.seq });
  }

  /** A layout id that changes when the geometry does, so frames stay matchable. */
  _noteLayout(mv) {
    if (!mv) return;
    const sig = JSON.stringify((mv.cells || []).map((c) => [c.index, c.x, c.y, c.w, c.h]));
    if (sig !== this._sig) {
      this._sig = sig;
      this.layoutId += 1;
    }
  }

  _layout(mv) {
    return {
      id: this.layoutId,
      width: mv ? mv.width : 0,
      height: mv ? mv.height : 0,
      cells: mv ? mv.cells || [] : [],
    };
  }

  // ---------------------------------------------------------------- http

  async _http(method, path, body, opts = {}) {
    const headers = {};
    if (this.token) headers.Authorization = "Bearer " + this.token;
    if (body !== undefined) headers["Content-Type"] = "application/json";
    const res = await fetch(new URL(path, this.base), {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body),
    });
    const text = await res.text();
    if (!res.ok) throw legacyError(res.status, text, method + " " + path);
    if (!text) return {};
    try {
      return JSON.parse(text);
    } catch {
      return { text };
    }
  }

  async _status() {
    const state = await this._http("GET", "/api/status");
    this.seq += 1;
    this._noteLayout(state.multiview);
    this.hooks.onEvent("snapshot", { seq: this.seq, state });
    this.hooks.onEvent("multiview.layout", this._layout(state.multiview));
    this.hooks.onEvent("flush", { seq: this.seq });
    return state;
  }

  // ---------------------------------------------------------------- calls

  async call(method, p = {}) {
    switch (method) {
      case "core.info": {
        const s = await this._status();
        return { core: "godwinmix", api_level: 0, legacy: true, backend: s.backend };
      }
      case "core.subscribe":
        return this.subscribe(p);
      case "core.api":
        throw notHere(method);
      case "core.shutdown":
        return this._http("POST", "/api/shutdown");

      case "program.take":
        return this._http("POST", "/api/take", {
          source: p.source ?? p.scene ?? null,
          at_running_time_ms: p.at_running_time_ms,
        });

      case "source.list":
        return { sources: (await this._status()).sources };
      case "source.add":
        // The old endpoint answers with nothing, so the caller waits for the
        // next status rather than being handed the id it just created.
        await this._http("POST", "/api/sources", {
          id: p.id ?? null,
          name: p.name ?? null,
          uri: p.uri,
          kind: p.kind ?? null,
          superimpose: p.superimpose ?? null,
        });
        return { pending: true };
      case "source.remove":
        return this._http("DELETE", `/api/sources/${encodeURIComponent(p.source)}`);
      case "source.audio.set":
        return this._http("POST", `/api/sources/${encodeURIComponent(p.source)}/audio`, audioBody(p));
      case "source.seek":
        return this._http("POST", `/api/sources/${encodeURIComponent(p.source)}/seek`, {
          position_ms: p.position_ms,
        });
      case "source.set":
        throw notHere(method);

      case "output.list":
        return { outputs: await this._http("GET", "/api/outputs") };
      case "output.add":
        return this._http("POST", "/api/outputs", {
          id: p.id,
          uri: p.uri,
          policy: p.policy || "own",
          queue_secs: p.queue_secs ?? 4.0,
        });
      case "output.remove":
        return this._http("DELETE", `/api/outputs/${encodeURIComponent(p.output)}`);
      case "output.reconnect":
        return this._http("POST", `/api/outputs/${encodeURIComponent(p.output)}/reconnect`);

      case "media.list":
        return this._http("GET", "/api/media");
      case "media.convert":
        return this._http("POST", `/api/media/${encodeURIComponent(p.name)}/convert`);
      case "media.remove":
        return this._http("DELETE", `/api/media/${encodeURIComponent(p.name)}`);

      case "adbreak.start":
        return this._http("POST", "/api/adbreak", {
          uri: p.uri,
          at_running_time_ms: p.at_running_time_ms,
          return_to: p.return_to,
        });
      case "adbreak.end":
        return this._http("POST", "/api/adbreak/end");

      case "agent.state":
        return this._http("GET", "/api/agent/state");

      case "golive":
        return this._http("POST", "/api/golive", p);

      default:
        throw notHere(method);
    }
  }

  upload(name, file, onProgress) {
    return httpUpload(this.base, this.token, name, file, onProgress);
  }

  snapshotUrl(name, width) {
    const u = new URL(`/api/snapshot/${encodeURIComponent(name)}.jpg`, this.base);
    if (width) u.searchParams.set("width", String(Math.round(width)));
    u.searchParams.set("t", String(Date.now()));
    if (this.token) u.searchParams.set("token", this.token);
    return u.toString();
  }
}

/** Only the field being moved is sent, which is what the old endpoint wants. */
function audioBody(p) {
  const body = {};
  if (p.gain_db !== undefined) body.gain = dbToLinear(p.gain_db);
  if (p.gain !== undefined) body.gain = p.gain;
  if (p.muted !== undefined) body.muted = p.muted;
  if (p.layers && p.layers.page !== undefined) body.page = p.layers.page;
  if (p.page !== undefined) body.page = p.page;
  if (p.layers && p.layers.media) body.media = p.layers.media;
  if (p.media !== undefined) body.media = p.media;
  return body;
}

function dbToLinear(db) {
  return Math.pow(10, db / 20);
}

function notHere(method) {
  return new RpcError(
    CODES.NO_METHOD,
    `this mixer does not have '${method}' yet (it is running the REST API, not /rpc). Update the mixer, or use the parts of the page that work without it.`,
    { method, legacy: true }
  );
}

/**
 * The old API answers a failure with plain text, or with `{"error": "..."}`, or
 * with a proxy's HTML page. Anything starting with `<` is thrown away in favour
 * of a sentence, because an HTML error page in a toast helps nobody.
 */
function legacyError(status, text, what) {
  let message = (text || "").trim();
  if (message.startsWith("<")) message = "";
  if (message.startsWith("{")) {
    try {
      const body = JSON.parse(message);
      if (typeof body.error === "string") message = body.error;
    } catch {
      /* leave it as it came */
    }
  }
  if (message.length > 300) message = message.slice(0, 300) + "…";
  if (!message) message = `${what} failed (${status})`;
  const code =
    status === 401 || status === 403
      ? CODES.NO_SCOPE
      : status === 404
        ? CODES.NOT_FOUND
        : status === 409
          ? CODES.WRONG_STATE
          : CODES.BAD_PARAMS;
  return new RpcError(code, message, { status, retryable: status >= 500 });
}
