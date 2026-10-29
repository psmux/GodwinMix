// This screen's own connection to the mixer.
//
// Not the full client the desk uses: a prompter on a phone has no use for
// the status document, the meters or the mosaic, so it subscribes to the
// prompter's two events and nothing more. The socket is the same `/rpc`
// with the same token, so a viewer token follows and an operator token can
// drive as well.

import { RpcTransport } from "../client/transport-rpc.js";

export const EVENTS = ["prompter.*"];

/** Does the mixer take this token? "ok", "token" (it wants one) or "down". */
export async function admitted(token) {
  try {
    const res = await fetch(new URL("/api/v1/core/info", location.origin), {
      headers: token ? { Authorization: "Bearer " + token } : {},
    });
    return res.status === 401 ? "token" : "ok";
  } catch {
    return "down";
  }
}

export class Link {
  /** hooks: {open(), close(), event(name, params)} */
  constructor(token, hooks) {
    this.transport = new RpcTransport({
      base: location.origin,
      token,
      hooks: {
        onOpen: () => hooks.open(),
        onClose: () => hooks.close(),
        onEvent: (name, params) => hooks.event(name, params),
        onFrame: () => {},
      },
    });
    // Restated by the transport on every open, so a reconnect subscribes
    // again by itself.
    this.transport.lastSubscribe = { events: EVENTS };
  }

  open() {
    this.transport.open();
  }

  call(method, params) {
    return this.transport.call(method, params || {});
  }
}
