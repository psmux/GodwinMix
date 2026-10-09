// The Rows view: every channel on one line, for a mixer with more channels
// than fit as cards. A Livebox user knows it as the bulk channel settings
// table. It draws from the same `channel.list` answer the cards do, read by
// the panel's own poll; it asks the core for nothing of its own. A press on a
// line goes back to the cards with that channel's card in view.

import { el } from "../../shell/dom.js";
import { keyed, write } from "./keyed.js";
import { fmtUptime } from "./model.js";
import { rowOf } from "./rows-model.js";

/** The list of lines. `open(id)` shows that channel's card. */
export function rowsView(open) {
  const rows = new Map();
  const node = el("div.chn-rows", { role: "list" });
  return {
    node,
    update(channels, now = Date.now()) {
      keyed(node, rows, channels.map((c) => rowOf(c, now)), (r) => r.id, () => channelRow(open));
    },
    tick() {
      for (const row of rows.values()) row.tick();
    },
  };
}

function channelRow(open) {
  let began = 0;
  let id = "";
  const dot = el("span.chn-dot");
  const name = el("strong.chn-rname");
  const status = el("span.chn-rstatus");
  const spec = el("span.chn-rspec");
  const uptime = el("span.chn-rtime", { title: "Publishing for" });
  const rings = el("span.chn-rrings");
  const sending = el("span.chn-rsend");
  const node = el("button.chn-row", { type: "button", role: "listitem" }, [
    dot,
    el("span.chn-rid", {}, [name, status]),
    spec,
    uptime,
    el("span.chn-rdest", {}, [rings, sending]),
  ]);
  node.onclick = () => open(id);

  function tick() {
    write(uptime, "textContent", began ? fmtUptime(Date.now() - began) : "");
  }

  return {
    node,
    tick,
    update(r) {
      id = r.id;
      began = r.began;
      node.dataset.state = r.state;
      write(dot, "className", "chn-dot" + (r.state === "waiting" ? "" : " " + r.state));
      write(name, "textContent", r.name);
      write(status, "textContent", r.status);
      write(spec, "textContent", r.spec);
      write(sending, "textContent", r.sending);
      // Red while a destination fails, with which one and why on hover.
      sending.classList.toggle("bad", r.failing.length > 0);
      write(sending, "title", r.failing.join("\n"));
      node.setAttribute("aria-label", [r.name, r.status, r.spec, r.sending].filter(Boolean).join(", "));
      const shape = r.rings.map((g) => g.state).join(" ");
      if (rings.dataset.shape !== shape) {
        rings.dataset.shape = shape;
        rings.replaceChildren(...r.rings.map((g) => el("span.chn-rring", { "data-state": g.state })));
      }
      [...rings.children].forEach((ring, i) => write(ring, "title", `${r.rings[i].label}: ${r.rings[i].state}`));
      tick();
    },
  };
}
