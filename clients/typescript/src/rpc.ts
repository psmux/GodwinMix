// JSON-RPC 2.0 over one WebSocket. One message per frame, text for control,
// binary for pictures.
//
// Request ids are per direction: our ids and the core's ids are separate spaces
// and may collide without ambiguity. The counter here only ever labels our own
// outgoing calls, and an incoming message with an id is a request from the core
// unless we have that id outstanding.
//
// The WebSocket is the platform's. Browsers have had one for fifteen years and
// Node has had a global one since 22. On Node 20 there is none, so pass one in:
//
//   import WebSocket from "ws";
//   connect({ base, token, webSocket: WebSocket as unknown as WebSocketLike });

import { RpcError } from "./errors.ts";

const RECONNECT_MS = [250, 500, 1000, 2000, 4000, 8000];

/** The part of the WebSocket API this client uses. */
export interface WebSocketLike {
  binaryType: string;
  readyState: number;
  onopen: ((ev: unknown) => void) | null;
  onmessage: ((ev: { data: unknown }) => void) | null;
  onclose: ((ev: unknown) => void) | null;
  onerror: ((ev: unknown) => void) | null;
  send(data: string): void;
  close(): void;
}

export type WebSocketFactory = new (url: string) => WebSocketLike;

export interface RpcHooks {
  onOpen?: () => void;
  onClose?: () => void;
  onNotify?: (method: string, params: Record<string, unknown>, id?: number) => void;
  onBinary?: (data: ArrayBuffer | Uint8Array) => void;
}

interface Waiting {
  resolve: (value: unknown) => void;
  reject: (error: RpcError) => void;
}

export class RpcSocket {
  url: string;
  hooks: RpcHooks;
  ws: WebSocketLike | null = null;
  nextId = 1;
  pending = new Map<number, Waiting>();
  closed = false;
  attempt = 0;
  ready = false;
  private make: WebSocketFactory;

  constructor(url: string, hooks: RpcHooks = {}, factory?: WebSocketFactory) {
    this.url = url;
    this.hooks = hooks;
    const platform = (globalThis as { WebSocket?: unknown }).WebSocket;
    const made = factory || (platform as WebSocketFactory | undefined);
    if (!made) {
      throw new Error(
        "no WebSocket in this runtime. Node 22 and later have one; on Node 20 pass " +
          "webSocket: (await import('ws')).WebSocket to connect().",
      );
    }
    this.make = made;
  }

  open(): void {
    this.closed = false;
    this.connect();
  }

  private connect(): void {
    let ws: WebSocketLike;
    try {
      ws = new this.make(this.url);
    } catch {
      this.retry();
      return;
    }
    ws.binaryType = "arraybuffer";
    this.ws = ws;

    ws.onopen = () => {
      this.attempt = 0;
      this.ready = true;
      this.hooks.onOpen?.();
    };

    ws.onmessage = (ev: { data: unknown }) => {
      if (typeof ev.data !== "string") {
        this.hooks.onBinary?.(ev.data as ArrayBuffer | Uint8Array);
        return;
      }
      let message: unknown;
      try {
        message = JSON.parse(ev.data);
      } catch {
        return;
      }
      this.dispatch(message);
    };

    ws.onclose = () => {
      this.ready = false;
      this.failPending("the connection to the mixer closed. It will be retried.");
      this.hooks.onClose?.();
      if (!this.closed) this.retry();
    };

    ws.onerror = () => {
      // onclose always follows, and that is where the retry lives.
    };
  }

  private dispatch(message: unknown): void {
    if (Array.isArray(message)) {
      for (const one of message) this.dispatch(one);
      return;
    }
    const msg = message as {
      id?: number;
      error?: { code: number; message: string; data?: Record<string, unknown> };
      result?: unknown;
      method?: string;
      params?: Record<string, unknown>;
    };
    if (msg.id !== undefined && this.pending.has(msg.id)) {
      const waiting = this.pending.get(msg.id)!;
      this.pending.delete(msg.id);
      if (msg.error) waiting.reject(new RpcError(msg.error.code, msg.error.message, msg.error.data));
      else waiting.resolve(msg.result);
      return;
    }
    if (msg.method) this.hooks.onNotify?.(msg.method, msg.params || {}, msg.id);
  }

  private retry(): void {
    if (this.closed) return;
    const wait = RECONNECT_MS[Math.min(this.attempt, RECONNECT_MS.length - 1)]!;
    this.attempt += 1;
    setTimeout(() => {
      if (!this.closed) this.connect();
    }, wait);
  }

  private failPending(why: string): void {
    for (const { reject } of this.pending.values()) {
      reject(new RpcError(-32010, why, { retryable: true }));
    }
    this.pending.clear();
  }

  /** Send a request and wait for its reply. */
  call(method: string, params?: Record<string, unknown>): Promise<unknown> {
    return new Promise((resolve, reject) => {
      if (!this.ws || this.ws.readyState !== 1) {
        reject(
          new RpcError(-32001, "not connected to the mixer yet. Wait for the connection to come back.", {
            retryable: true,
          }),
        );
        return;
      }
      const id = this.nextId++;
      this.pending.set(id, { resolve, reject });
      this.ws.send(JSON.stringify({ jsonrpc: "2.0", id, method, params: params || {} }));
    });
  }

  /** Send a notification: no id, no reply expected. */
  notify(method: string, params?: Record<string, unknown>): void {
    if (!this.ws || this.ws.readyState !== 1) return;
    this.ws.send(JSON.stringify({ jsonrpc: "2.0", method, params: params || {} }));
  }

  close(): void {
    this.closed = true;
    this.failPending("the client closed the connection");
    if (this.ws) this.ws.close();
    this.ws = null;
  }
}
