// The client a sandboxed panel uses, inside its iframe.
//
// A sandboxed panel is an HTML file with no same origin access. It cannot reach
// the page, the token or storage. What it gets is this: the same method names,
// the same events, the same state document, proxied over postMessage by the
// shell. A panel written against this and a panel written as a custom element
// call exactly the same protocol, which is the point of having two tiers.
//
//   <script type="module">
//     import { connectPanel } from "/client/sandbox-client.js";
//     const client = await connectPanel();
//     client.onRender(state => { ... });
//     await client.call("program.take", {source: "cam1"});
//   </script>

const pending = new Map();
let nextId = 1;

export async function connectPanel() {
  const listeners = new Map();
  let state = {};
  let config = {};
  let capabilities = {};

  function emit(name, arg) {
    const set = listeners.get(name);
    if (!set) return;
    for (const fn of set) {
      try {
        fn(arg);
      } catch (e) {
        console.error(e);
      }
    }
  }

  window.addEventListener("message", (e) => {
    // The only sender that can reach this frame is the shell that made it.
    const msg = e.data;
    if (!msg || msg.jsonrpc !== "2.0") return;
    if (msg.id !== undefined && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) {
        const err = new Error(msg.error.message);
        err.code = msg.error.code;
        err.data = msg.error.data;
        reject(err);
      } else {
        resolve(msg.result);
      }
      return;
    }
    if (typeof msg.method === "string" && msg.method.startsWith("event/")) {
      const name = msg.method.slice(6);
      if (name === "state") {
        state = msg.params.state;
        emit("render", state);
        return;
      }
      emit("event", { name, params: msg.params });
      emit(name, msg.params);
    }
  });

  function send(method, params) {
    return new Promise((resolve, reject) => {
      const id = nextId++;
      pending.set(id, { resolve, reject });
      parent.postMessage({ jsonrpc: "2.0", id, method, params: params || {} }, "*");
    });
  }

  const hello = await send("panel.hello", {});
  state = hello.state || {};
  config = hello.config || {};
  capabilities = hello.capabilities || {};

  return {
    get state() {
      return state;
    },
    get config() {
      return config;
    },
    get capabilities() {
      return capabilities;
    },
    call: send,
    /** Ask for an expensive stream. The shell releases it when the panel goes. */
    subscribe(ext) {
      return send("core.subscribe", { ext });
    },
    /** Tell the shell how tall the panel wants to be. */
    resize(height) {
      parent.postMessage({ jsonrpc: "2.0", method: "panel.resize", params: { height } }, "*");
    },
    on(name, fn) {
      if (!listeners.has(name)) listeners.set(name, new Set());
      listeners.get(name).add(fn);
      return () => listeners.get(name).delete(fn);
    },
    onRender(fn) {
      const off = this.on("render", fn);
      fn(state);
      return off;
    },
  };
}
