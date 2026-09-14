// The native transport: JSON-RPC 2.0 over `/rpc`.
//
// This is the one the UI is written against. Everything else in the client is
// arranged so that the legacy adapter can stand in its place without a panel
// noticing, and so that the adapter can be deleted with no other edit.

import { RpcSocket } from "./rpc.js";
import { parseFrame } from "./frames.js";

export class RpcTransport {
  /** @param {{base: string, token: ?string, hooks: object}} opts */
  constructor(opts) {
    this.name = "rpc";
    this.base = opts.base;
    this.token = opts.token;
    this.hooks = opts.hooks;
    this.socket = null;
  }

  /** Whatever `core.subscribe` last accepted, so a reconnect can restate it. */
  lastSubscribe = null;

  url() {
    const u = new URL("/rpc", this.base);
    u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
    if (this.token) u.searchParams.set("token", this.token);
    return u.toString();
  }

  open() {
    this.socket = new RpcSocket(this.url(), {
      onOpen: () => {
        this.hooks.onOpen();
        if (this.lastSubscribe) this.socket.call("core.subscribe", this.lastSubscribe).catch(() => {});
      },
      onClose: () => this.hooks.onClose(),
      onNotify: (method, params) => {
        if (!method.startsWith("event/")) return;
        this.hooks.onEvent(method.slice(6), params);
      },
      onBinary: (buf) => {
        const frame = parseFrame(buf);
        if (frame) this.hooks.onFrame(frame);
      },
    });
    this.socket.open();
  }

  close() {
    if (this.socket) this.socket.close();
    this.socket = null;
  }

  subscribe(spec) {
    this.lastSubscribe = spec;
    return this.socket ? this.socket.call("core.subscribe", spec) : Promise.resolve({});
  }

  call(method, params) {
    if (!this.socket) return Promise.reject(new Error("not connected"));
    return this.socket.call(method, params);
  }

  /**
   * Uploads go over HTTP even here: a WebSocket frame is capped at 4 MiB and a
   * clip is not, and only XHR reports progress on the way up.
   */
  upload(name, file, onProgress) {
    return httpUpload(this.base, this.token, name, file, onProgress);
  }

  snapshotUrl(name, width) {
    const u = new URL(`/api/v1/snapshot/${encodeURIComponent(name)}.jpg`, this.base);
    if (width) u.searchParams.set("width", String(Math.round(width)));
    u.searchParams.set("t", String(Date.now()));
    if (this.token) u.searchParams.set("token", this.token);
    return u.toString();
  }
}

/** Shared by both transports: one file, streamed, with a progress callback. */
export function httpUpload(base, token, name, file, onProgress) {
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    const u = new URL("/api/media/upload", base);
    u.searchParams.set("name", name);
    xhr.open("POST", u.toString());
    xhr.setRequestHeader("Content-Type", "application/octet-stream");
    if (token) xhr.setRequestHeader("Authorization", "Bearer " + token);
    xhr.upload.onprogress = (e) => {
      if (onProgress && e.lengthComputable) onProgress(e.loaded / e.total);
    };
    xhr.onload = () => {
      if (xhr.status >= 200 && xhr.status < 300) {
        let body = {};
        try {
          body = JSON.parse(xhr.responseText);
        } catch {
          /* an empty 200 is still a success */
        }
        resolve(body);
      } else {
        reject(new Error(xhr.responseText || `upload failed (${xhr.status})`));
      }
    };
    xhr.onerror = () => reject(new Error("the upload could not reach the mixer"));
    xhr.send(file);
  });
}
