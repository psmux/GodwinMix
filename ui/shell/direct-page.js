// A page on a direct show: one input straight to its outputs, with no mixer
// behind the page. The station stands in for core.info, and the status the
// page renders carries no programme and no outputs, so on the tester's
// direct show the tab header said "black" and "No destinations", and the
// Outputs panel "Nothing is being sent anywhere", while its HLS output was
// live. Here the show's own outputs are read from show.list and show.stats
// and put in the page's state as `direct`, so the header and the Outputs
// panel draw them on the render they already have, with nothing more in
// the page every volunteer opens. Fetched only on a station's page, and it
// reads nothing more unless the show is direct.

import { el } from "./dom.js";

const STATS_MS = 2000;
// show.list again every this many stats reads, and on any show event.
const LIST_EVERY = 5;

/** An input that is not arriving, by the show's own alarms. */
const NO_INPUT = new Set(["no-input", "stall"]);

/**
 * The show's outputs as the header's pill reads a mixer's: id, state, the
 * error as `{message}`, the key and the rate. One that is off is stopped.
 */
export function directOutputs(show, stats) {
  const by = new Map(((stats && stats.outputs) || []).map((o) => [o.id, o]));
  return ((show && show.outputs) || []).map((o) => {
    const st = by.get(o.id);
    const state = o.enabled === false ? "stopped" : (st && st.state) || o.state || "waiting";
    return {
      id: o.id,
      type: "direct",
      label: o.label || o.id,
      uri_host: o.uri_host,
      state,
      error: o.error ? { message: o.error } : null,
      has_key: o.has_key,
      kbps: (st && st.kbps) || o.kbps || 0,
    };
  });
}

/** What the header's tally says for a direct show: its input, live or not. */
export function directTally(show, stats) {
  const alarms = ((show && show.health && show.health.alarms) || []).map((a) => a.kind);
  const running = !!show && (show.state === "running" || show.state === "live");
  const kbps = (stats && stats.input && stats.input.kbps) || 0;
  const on = running && kbps > 0 && !alarms.some((k) => NO_INPUT.has(k));
  return { on, label: on ? "direct, input live" : running ? "direct, no input" : `direct, ${show ? show.state : "stopped"}` };
}

/** Follow the page's show, if it is direct. Answers a stop, or null. */
export async function followDirect(client) {
  const list = await client.call("show.list", {}).catch(() => null);
  const id = new URLSearchParams(location.search).get("show") || (list && list.current);
  let show = list && (list.shows || []).find((s) => s.id === id);
  if (!show || show.compositing !== false) return null;
  let stats = null;
  let reads = 0;
  const publish = () => {
    client.store.patch({ direct: { show, stats, outputs: directOutputs(show, stats), tally: directTally(show, stats) } });
  };
  const readList = async () => {
    const got = await client.call("show.list", {}).catch(() => null);
    const fresh = got && (got.shows || []).find((s) => s.id === show.id);
    if (fresh) show = fresh;
    publish();
  };
  const readStats = async () => {
    const got = await client.call("show.stats", { ids: [show.id] }).catch(() => null);
    stats = (got && got.shows && got.shows[0]) || stats;
    if (++reads % LIST_EVERY === 0) return readList();
    publish();
  };
  publish();
  readStats();
  const timer = setInterval(() => document.hidden || readStats(), STATS_MS);
  const off = client.on("event", (e) => /^show\./.test((e && e.name) || "") && readList());
  return () => {
    clearInterval(timer);
    off();
    client.store.patch({ direct: null });
  };
}

/**
 * The Outputs panel on a direct show, which has no output in its status:
 * the show's own, drawn again only when what they say changes, for the
 * reason the panel's `render` gives.
 */
export function intoOutputs(panel, d) {
  const said = JSON.stringify([d.show.name, d.outputs]);
  panel.count.textContent = d.outputs.length ? String(d.outputs.length) : "";
  if (said === panel.directShape || panel.client.state.direct !== d) return;
  panel.directShape = said;
  panel.rows.clear();
  panel.list.replaceChildren(directSummary(panel.client, d));
}

/** The Outputs panel's body on a direct show: each output, and the detail. */
export function directSummary(client, d) {
  const rows = d.outputs.map((o) => el(`div.row.direct-out`, { style: { gap: "8px", alignItems: "center" } }, [
    el(`span.dot${o.state === "live" ? ".live" : ""}`),
    el("strong", { text: o.label }),
    el("span.sm.dim", { text: [o.state, o.kbps ? `${(o.kbps / 1000).toFixed(1)} Mb/s` : "", o.error && o.state !== "live" ? o.error.message : ""].filter(Boolean).join(" · ") }),
  ]));
  const n = d.outputs.length;
  const live = d.outputs.filter((o) => o.state === "live").length;
  const words = n
    ? `${d.show.name} is a direct show: its input goes straight to ${n === 1 ? "one output" : `${n} outputs`}, ${live} live.`
    : `${d.show.name} is a direct show with no outputs yet. Its input is read but sent nowhere.`;
  return el("div.col", { style: { gap: "6px" } }, [
    el("p.dim.sm", { text: words, style: { margin: 0 } }),
    ...rows,
    el("div.row", {}, [el("button.btn", {
      type: "button",
      text: "Outputs and settings…",
      title: "Its outputs, with their links, its input and its alarms",
      onclick: () => import("../panels/wall/detail.js").then((m) => m.showDetail(client, d.show)),
    })]),
  ]);
}
