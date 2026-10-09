// What is running: every stream, recording, watch link and incoming encoder,
// each with a Stop, and one Stop all at the bottom. Opened from the header's
// pill on a desk and on a phone alike, and from the banner.
//
// While it is open, and only then, it reads the outputs and `channel.list`
// every two seconds, for the bit rates no event carries. Closing it stops
// the reading.

import { el, clear, fmtDuration } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { runningThings, outgoing, stopAllLabel } from "./running-model.js";
import { stopItem, stopAll } from "./stop.js";

const EVERY_MS = 2000;

/** kb/s by output id, from how far `bytes_out` moved since the last read. */
export function ratesFrom(outputs, before, now) {
  const rates = {};
  for (const o of outputs || []) {
    const was = before.get(o.id);
    if (was && typeof o.bytes_out === "number" && now > was.at) {
      rates[o.id] = Math.max(0, Math.round(((o.bytes_out - was.bytes) * 8) / (now - was.at)));
    }
    if (typeof o.bytes_out === "number") before.set(o.id, { bytes: o.bytes_out, at: now });
  }
  return rates;
}

export function openRunning(client) {
  let channels = [];
  let rates = {};
  const seen = new Map();
  const list = el("div.col.running-list");
  const all = el("button.btn.danger", { text: "Stop all streaming", onclick: () => everything() });
  const m = modal({
    title: "What is running",
    body: list,
    footer: [all, el("button.btn", { text: "Close", onclick: () => m.close() })],
    onClose: () => clearInterval(timer),
  });
  m.el.classList.add("running-dialog");

  async function read() {
    try {
      if (client.refreshOutputs) await client.refreshOutputs();
      const answer = await client.call("channel.list", {});
      channels = (answer && answer.channels) || [];
    } catch {
      // What was read last stays on screen.
    }
    rates = ratesFrom((client.state || {}).outputs, seen, performance.now());
    paint();
  }

  function paint() {
    const items = runningThings(client.state, channels, rates);
    clear(list);
    if (!items.length) {
      list.append(el("p.dim", { text: "Nothing is running. Nothing is being streamed, recorded or received.", style: { margin: 0 } }));
    }
    for (const item of items) list.append(row(item));
    all.textContent = stopAllLabel(items);
    all.disabled = !outgoing(items).length;
  }

  function row(item) {
    const bits = [item.state, item.where];
    if (item.live && item.since_secs !== undefined) bits.push("for " + fmtDuration(item.since_secs));
    if (item.kbps) bits.push(`${item.kbps.toLocaleString()} kb/s`);
    const stop = el("button.btn.danger", {
      text: item.kind === "ingest" ? "Turn away" : "Stop",
      "aria-label": `Stop ${item.title}`,
      onclick: async () => {
        stop.disabled = true;
        if (await stopItem(client, item)) await read();
        else stop.disabled = false;
      },
    });
    return el("div.row.running-row", { "data-key": item.key }, [
      el("span.dot" + (item.live ? ".live" : ".connecting")),
      el("div.col.grow", {}, [el("strong", { text: item.title }), el("span.sm.dim", { text: bits.join(" · ") })]),
      stop,
    ]);
  }

  async function everything() {
    all.disabled = true;
    await stopAll(client, runningThings(client.state, channels, rates));
    await read();
  }

  const timer = setInterval(read, EVERY_MS);
  paint();
  read();
  return m;
}
