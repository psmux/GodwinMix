// The wall's top bar: the counts, the filter, the alarm and group choices,
// rows or tiles, Add shows and close.

import { el } from "../../shell/dom.js";
import { ALARMS, kbps } from "./model.js";

export function topBar(opts, act) {
  const summary = el("p.wl-summary", { "aria-live": "polite" });
  const filter = el("input.wl-filter", { type: "search", placeholder: "Filter shows, inputs, outputs", "aria-label": "Filter shows", value: opts.text || "" });
  const alarm = el("select.wl-select", { "aria-label": "Which shows" }, [
    el("option", { value: "", text: "All shows" }),
    el("option", { value: "any", text: "Alarm or warning" }),
    ...Object.entries(ALARMS).map(([k, v]) => el("option", { value: k, text: v })),
  ]);
  alarm.value = opts.alarm || "";
  const toggle = (word, key, pressed) => el("button.btn.wl-tog", { type: "button", "data-top": key, "aria-pressed": String(!!pressed), text: word });
  const group = toggle("Group by state", "group", opts.group);
  const rowsB = toggle("Rows", "rows", opts.mode !== "tiles");
  const tilesB = toggle("Tiles", "tiles", opts.mode === "tiles");
  const node = el("header.wl-top", {}, [
    el("div.wl-titlebar", {}, [
      el("h2", { text: "Shows" }), summary, el("span.grow"),
      el("button.btn.primary.wl-addmany", { type: "button", "data-top": "add", text: "Add shows" }),
      el("button.btn.icon", { type: "button", "data-top": "close", text: "×", title: "Close the wall (Escape)", "aria-label": "Close the wall" }),
    ]),
    el("div.wl-tools", {}, [filter, alarm, group, el("span.wl-seg", { role: "group", "aria-label": "Layout" }, [rowsB, tilesB])]),
  ]);
  node.addEventListener("click", (e) => {
    const b = e.target.closest("[data-top]");
    if (b) act(b.dataset.top);
  });
  filter.addEventListener("input", () => act("text", filter.value));
  alarm.addEventListener("change", () => act("alarm", alarm.value));
  return {
    node, filter,
    sync(o) {
      group.setAttribute("aria-pressed", String(!!o.group));
      rowsB.setAttribute("aria-pressed", String(o.mode !== "tiles"));
      tilesB.setAttribute("aria-pressed", String(o.mode === "tiles"));
    },
    counts(s) { summary.replaceChildren(...words(s)); },
  };
}

function words(s) {
  const n = (v, word, cls = "") => el(`span.wl-stat${cls}`, {}, [el("strong", { text: String(v) }), ` ${word}`]);
  const parts = [
    n(s.shows, s.shows === 1 ? "show" : "shows"),
    n(s.live, "live"),
    n(s.alarm, "in alarm", s.alarm ? ".bad" : ""),
    el("span.wl-stat", { title: s.partial ? "Summed over the shows read so far" : "" }, [`in `, el("strong", { text: kbps(s.inK) || "0 kb/s" }), s.partial ? "*" : ""]),
    el("span.wl-stat", {}, ["out ", el("strong", { text: kbps(s.outK) || "0 kb/s" })]),
  ];
  if (s.cpu != null) parts.push(n(`${s.cpu}%`, "CPU", s.cpu > 85 ? ".bad" : ""));
  if (s.gpu != null) parts.push(n(`${s.gpu}%`, "GPU", s.gpu > 85 ? ".bad" : ""));
  return parts;
}
