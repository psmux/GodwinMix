// The sandboxed tier: a panel in an <iframe sandbox="allow-scripts"> talking the
// same JSON-RPC over postMessage.
//
// The frame has an opaque origin, so `event.origin` is the string "null" and
// checking it proves nothing. The check that matters is
// `event.source === frame.contentWindow`, which a frame cannot forge; replies
// go back with "*" because an opaque origin cannot be named any other way.
//
// A sandboxed panel may call methods, subscribe and read state. It may not
// touch the page, read the token, or hold an `ext` stream the shell is not
// counting: that bookkeeping stays here, so a panel removed stops costing.

const ALLOWED_PREFIXES = ["core.", "program.", "source.", "output.", "scene.", "media.", "adbreak.", "plugin.", "agent.", "tool.", "task."];

export class SandboxHost {
  /**
   * @param {HTMLIFrameElement} frame
   * @param {object} client
   * @param {object} config
   * @param {object} spec
   */
  constructor(frame, client, config, spec) {
    this.frame = frame;
    this.client = client;
    this.config = config;
    this.spec = spec;
    this.wants = new Map();
    this.offs = [];
    this.onMessage = (e) => this._message(e);
    window.addEventListener("message", this.onMessage);

    // Everything the panel would have got from setClient, pushed instead.
    this.offs.push(
      client.onRender((state) => this._send({ method: "event/state", params: { state } })),
      client.on("event", ({ name, params }) => this._send({ method: "event/" + name, params })),
      client.on("open", () => this._send({ method: "event/open", params: {} })),
      client.on("close", () => this._send({ method: "event/close", params: {} }))
    );
  }

  destroy() {
    window.removeEventListener("message", this.onMessage);
    for (const off of this.offs) off();
    for (const want of this.wants.values()) want.release();
    this.wants.clear();
    this.frame.remove();
  }

  _send(msg) {
    const win = this.frame.contentWindow;
    if (!win) return;
    // An opaque origin cannot be addressed by name, so "*" is the only target
    // that works. It is safe here because the frame is one we created, its
    // document comes from our own origin's /plugins/ path, and it holds no
    // secret of ours to leak back.
    win.postMessage(Object.assign({ jsonrpc: "2.0" }, msg), "*");
  }

  async _message(e) {
    if (!this.frame.contentWindow || e.source !== this.frame.contentWindow) return;
    const msg = e.data;
    if (!msg || msg.jsonrpc !== "2.0" || typeof msg.method !== "string") return;

    // The handshake a panel does first, so it can render before any event.
    if (msg.method === "panel.hello") {
      this._send({
        id: msg.id,
        result: {
          config: this.config,
          state: this.client.state,
          capabilities: this.client.capabilities,
          panel: { id: this.spec.id, title: this.spec.title },
        },
      });
      return;
    }

    if (msg.method === "panel.resize") {
      const h = Number(msg.params && msg.params.height);
      if (Number.isFinite(h) && h > 0 && h < 4000) this.frame.style.height = Math.round(h) + "px";
      return;
    }

    if (msg.method === "core.subscribe") {
      this._subscribe(msg.params && msg.params.ext);
      if (msg.id !== undefined) this._send({ id: msg.id, result: {} });
      return;
    }

    if (msg.id === undefined) return; // a notification we do not act on

    if (!ALLOWED_PREFIXES.some((p) => msg.method.startsWith(p))) {
      this._send({
        id: msg.id,
        error: { code: -32002, message: `a sandboxed panel may not call '${msg.method}'. Ask the operator to mark this plugin trusted.`, data: { method: msg.method } },
      });
      return;
    }

    try {
      const result = await this.client.call(msg.method, msg.params || {});
      this._send({ id: msg.id, result });
    } catch (err) {
      this._send({ id: msg.id, error: { code: err.code || -32603, message: err.message, data: err.data || {} } });
    }
  }

  /** The panel's ext wishes, held by the shell so removal actually releases. */
  _subscribe(ext) {
    const wanted = ext && typeof ext === "object" ? ext : {};
    for (const [key, want] of this.wants) {
      if (!(key in wanted) || wanted[key] === false) {
        want.release();
        this.wants.delete(key);
      }
    }
    for (const [key, value] of Object.entries(wanted)) {
      if (value === false) continue;
      if (this.wants.has(key)) this.wants.get(key).update(value);
      else this.wants.set(key, this.client.want(key, value));
    }
  }
}
