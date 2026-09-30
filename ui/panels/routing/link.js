// A line to one show's own methods while its part of the routing view is on
// screen. The show this page is on is the page's own client; any other is a
// second socket with `?show=<id>`, as the station routes it, closed the
// moment its group scrolls away or the view closes.

import { RpcSocket } from "../../client/rpc.js";

const EVENTS = ["output.*", "source.*", "rendition.*", "tally"];

const late = (ms, what) => new Promise((_, reject) => setTimeout(() => reject(Object.assign(new Error(`${what} got no answer in ${ms / 1000} seconds. The show may be busy or stopped; it is asked again when it comes back.`), { code: -32010, data: { retryable: true } })), ms));

/**
 * @returns {{call: (m: string, p?: object) => Promise<any>, on: (fn: (name: string, params: object) => void) => () => void, close: () => void, client: object}}
 */
export function showLink(client, id, current) {
  if (id === current) return pageLink(client);
  if (client.linkShow) return client.linkShow(id);
  return socketLink(client, id);
}

function pageLink(client) {
  const held = client.listen ? client.listen("rendition.*") : () => {};
  return {
    client,
    call: (m, p) => client.call(m, p || {}),
    on: (fn) => client.on("event", ({ name, params }) => fn(name, params)),
    close: held,
  };
}

function socketLink(client, id) {
  const t = client.transport || {};
  const u = new URL("/rpc", t.base || location.origin);
  u.protocol = u.protocol === "https:" ? "wss:" : "ws:";
  if (t.token) u.searchParams.set("token", t.token);
  u.searchParams.set("show", id);
  const fns = new Set();
  let opened;
  const ready = new Promise((resolve) => (opened = resolve));
  const sock = new RpcSocket(u.toString(), {
    onOpen: () => {
      opened();
      sock.call("core.subscribe", { events: EVENTS }).catch(() => {});
    },
    onNotify: (method, params) => {
      if (method.startsWith("event/")) for (const fn of fns) fn(method.slice(6), params);
    },
  });
  sock.open();
  const link = {
    call: (m, p) => Promise.race([ready.then(() => sock.call(m, p || {})), late(5000, m)]),
    on: (fn) => (fns.add(fn), () => fns.delete(fn)),
    close: () => sock.close(),
  };
  // Enough of a client for the Add destination form, which only calls.
  link.client = { call: link.call, on: () => () => {}, listen: () => () => {}, state: { outputs: [] }, refreshOutputs: async () => {}, transport: t };
  return link;
}

/** One show's outputs, sources and plan, and which sources are on air. */
export async function readDetail(link) {
  const quiet = (p) => p.catch((e) => (e && e.code !== -32601 && console.debug("routing", e), null));
  const [outputs, sources, plan] = await Promise.all([
    quiet(link.call("output.list", {})),
    quiet(link.call("source.list", {})),
    quiet(link.call("rendition.plan", {})),
  ]);
  const s = link.client && link.client.state;
  const tally = { ...(s && s.tally) };
  if (s && s.program && !tally[s.program]) tally[s.program] = "program";
  return { detail: { outputs: list(outputs, "outputs"), sources: list(sources, "sources"), tally: link.tally || tally }, plan };
}

/** `/rpc` answers with the array; some answers wrap it in an object. */
function list(answer, field) {
  if (Array.isArray(answer)) return answer;
  return (answer && answer[field]) || [];
}
