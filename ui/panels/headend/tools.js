// The bar above the grid: search, problems only, which group, the order,
// and a legend that is also a filter: how many channels are in each state,
// and a click on one shows only those.

import { el } from "../../shell/dom.js";
import { STATES } from "./model.js";

const SORT_WORDS = [["number", "Number"], ["name", "Name"], ["state", "Worst first"], ["rate", "Highest rate"], ["errors", "Most errors"]];

export function toolBar(o, act) {
  const search = el("input.he-search", { type: "search", placeholder: "Find a channel, a number or an address", "aria-label": "Find a channel", value: o.text || "" });
  const problems = el("button.btn.he-tog", { type: "button", "data-tool": "problems", "aria-pressed": String(!!o.problems), text: "Problems only" });
  const group = el("select.he-select", { "aria-label": "Input group" });
  const sort = el("select.he-select", { "aria-label": "Order" }, SORT_WORDS.map(([v, t]) => el("option", { value: v, text: t })));
  sort.value = o.sort || "number";
  const legend = el("div.he-legend", { role: "group", "aria-label": "Channels by state" });
  const node = el("div.he-tools", {}, [search, problems, group, sort, legend]);
  search.addEventListener("input", () => act("text", search.value));
  group.addEventListener("change", () => act("group", group.value));
  sort.addEventListener("change", () => act("sort", sort.value));
  node.addEventListener("click", (e) => {
    const b = e.target.closest("[data-tool], [data-state]");
    if (!b) return;
    if (b.dataset.tool) act(b.dataset.tool);
    else act("state", b.dataset.state);
  });
  return {
    node, search,
    draw(opts, counts, keys) {
      problems.setAttribute("aria-pressed", String(!!opts.problems));
      const want = ["", ...keys].join("|");
      if (group.dataset.keys !== want) {
        group.dataset.keys = want;
        group.replaceChildren(el("option", { value: "", text: "Every input" }), ...keys.map((k) => el("option", { value: k, text: k })));
      }
      group.value = opts.group || "";
      const key = STATES.map(([k]) => counts[k]).join(",") + opts.state;
      if (legend.dataset.key === key) return;
      legend.dataset.key = key;
      legend.replaceChildren(...STATES.filter(([k]) => counts[k] || opts.state === k).map(([k, word]) => el(`button.he-chipstate.s-${k}`, {
        type: "button", "data-state": k, "aria-pressed": String(opts.state === k), title: opts.state === k ? "Show every state again" : `Only ${word.toLowerCase()} channels`,
      }, [el("span.he-dotstate"), el("strong", { text: String(counts[k]) }), ` ${word.toLowerCase()}`])));
    },
  };
}
