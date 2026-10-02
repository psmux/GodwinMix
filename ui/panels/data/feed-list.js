// The feeds this show reads and what each is bound to, with its state at a
// glance: a green dot and when it was read, or a red one and why not.

import { el } from "../../shell/dom.js";
import { confirmModal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { describeTarget, shortValue, stateLine } from "./model.js";

/**
 * @param {{call: Function}} client
 * @param {{feeds: object[], bindings: object[]}} list  what `feed.list` answered
 * @param {() => void} changed  called after a button changed something
 */
export function feedList(client, list, changed) {
  if (!list.feeds.length) {
    return el("p.dim.sm.feeds-empty", { text: "No feeds yet. Paste an address below and press Try." });
  }
  const act = (method, params) => client.call(method, params).then(changed, (e) => toast({ text: e.message || String(e) }));
  const remove = async (id) => {
    if (await confirmModal(`Remove the feed ${id} and its bindings? What they wrote stays on air.`, "Remove")) act("feed.remove", { id });
  };
  return el(
    "div.col.feed-list",
    { style: { gap: "10px" } },
    list.feeds.map((feed) => {
      const line = stateLine(feed);
      const paused = feed.paused || feed.state === "paused";
      const bindings = list.bindings.filter((b) => b.feed === feed.id);
      return el("div.col.feed-row", { "data-feed": feed.id, style: { gap: "4px" } }, [
        el("div.row", { style: { gap: "8px", alignItems: "center" } }, [
          el(`span.dot${line.dot ? "." + line.dot : ""}`),
          el("strong", { text: feed.id }),
          el("span.dim.sm.grow", { text: `${feed.kind}, ${feed.address}` }),
          el("button.btn.sm", { text: "Fetch now", disabled: paused, onclick: () => act("feed.refresh", { id: feed.id }) }),
          el("button.btn.sm", { text: paused ? "Resume" : "Pause", onclick: () => act("feed.pause", { id: feed.id, paused: !paused }) }),
          el("button.btn.sm", { text: "Remove", onclick: () => remove(feed.id) }),
        ]),
        el(`p.sm.feed-state${line.dot === "failed" ? ".bad" : ".dim"}`, { text: line.text, style: { margin: "0 0 0 15px" } }),
        ...bindings.map((b) => bindingRow(b, act)),
      ]);
    })
  );
}

function bindingRow(b, act) {
  const said = b.last_error ? `Not written: ${b.last_error}` : b.value !== undefined ? `Wrote ${shortValue(b.value)}` : "Nothing written yet";
  return el("div.row.binding-row", { "data-binding": b.id, style: { gap: "8px", marginLeft: "15px", alignItems: "center" } }, [
    el("span.sm", { text: `${b.select || "(all)"} → ${describeTarget(b.to)}` }),
    el(`span.sm.grow${b.last_error ? ".bad" : ".dim"}`, { text: said }),
    el("button.btn.sm", { text: b.paused ? "Resume" : "Hold", onclick: () => act("feed.binding.pause", { id: b.id, paused: !b.paused }) }),
    el("button.btn.sm", { text: "Unbind", onclick: () => act("feed.binding.remove", { id: b.id }) }),
  ]);
}
