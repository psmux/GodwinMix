// JSON-RPC 2.0 over one WebSocket. One message per frame, text for control,
// binary for pictures.
//
// Request ids are per direction: our ids and the core's ids are separate
// spaces and may collide without ambiguity (03 section 6). So the id counter
// here only ever labels our own outgoing calls, and an incoming message with
// an id is a request from the core, never a reply to one of ours unless we
// have that id outstanding.

import { RpcError } from "./errors.js";

const RECONNECT_MS = [250, 500, 1000, 2000, 4000, 8000];

export class RpcSocket {
  /**
   * @param {string} url     ws:// or wss:// address, token already in the query
   * @param {object} hooks   {onNotify, onBinary, onOpen, onClose}
   */
  constructor(url, hooks = {}) {
    this.url = url;
    this.hooks = hooks;
    this.ws = null;
    this.nextId = 1;
    this.pending = new Map();
    this.closed = false;
    this.attempt = 0;
    this.ready = false;
  }

  open() {
    this.closed = false;
    this._connect();
  }

  _connect() {
    let ws;
    try {
      ws = new WebSocket(this.url);
    } catch (e) {
      this._retry();
      return;
    }
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
      this.attempt = 0;
      this.ready = true;
      this.hooks.onOpen && this.hooks.onOpen();
    };

    ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") {
        this.hooks.onBinary && this.hooks.onBinary(ev.data);
        return;
      }
      let msg;
      try {
        msg = JSON.parse(ev.data);
      } catch {
        return;
      }
      this._dispatch(msg);
    };

    ws.onclose = () => {
      this.ready = false;
      this._failPending("the connection to the mixer closed. It will be retried.");
      this.hooks.onClose && this.hooks.onClose();
      if (!this.closed) this._retry();
    };

    ws.onerror = () => {
      // onclose always follows, and that is where the retry lives.
    };
  }

  _dispatch(msg) {
    if (Array.isArray(msg)) {
      for (const m of msg) this._dispatch(m);
      return;
    }
    if (msg.id !== undefined && this.pending.has(msg.id)) {
      const { resolve, reject } = this.pending.get(msg.id);
      this.pending.delete(msg.id);
      if (msg.error) reject(new RpcError(msg.error.code, msg.error.message, msg.error.data));
      else resolve(msg.result);
      return;
    }
    if (msg.method) this.hooks.onNotify && this.hooks.onNotify(msg.method, msg.params || {}, msg.id);
  }

  _retry() {
    if (this.closed) return;
    const wait = RECONNECT_MS[Math.min(this.attempt, RECONNECT_MS.length - 1)];
    this.attempt += 1;
    setTimeout(() => {
      if (!this.closed) this._connect();
    }, wait);
  }

  _failPending(why) {
    for (const { reject } of this.pending.values()) reject(new RpcError(-32010, why, { retryable: true }));
    this.pending.clear();
  }

  /** Send a request and wait for its reply. */
  call(method, params) {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
        reject(new RpcError(-32001, "not connected to the mixer yet. Wait for the connection to come back.", { retryable: true }));
        return;
      }
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: params || {} }));
    });
  }

  /** Send a notification: no id, no reply expected. */
  notify(method, params) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    this.ws.send(JSON.stringify({ jsonrpc: "2.0", method, params: params || {} }));
  }

  /** Answer a request the core made of us. */
  reply(id, result, error) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
    const body = { jsonrpc: "2.0", id };
    if (error) body.error = error;
    else body.result = result === undefined ? {} : result;
    this.ws.send(JSON.stringify(body));
  }

  close() {
    this.closed = true;
    this._failPending("the client closed the connection");
    if (this.ws) this.ws.close();
    this.ws = null;
  }
}
