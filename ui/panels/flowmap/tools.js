// The bar over the map: its name and the station's totals, a search, the
// filters (problems only, one input, one kind of output), and fit, full
// screen and close. And the legend in the corner, which says what each
// colour means so colour is never the only thing that says it.

import { el, svg } from "../../shell/dom.js";
import { FAMILIES, rate } from "./words.js";

const ICON = {
  fit: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5M9 12h6M12 9v6",
  full: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
  close: "M6 6l12 12M18 6L6 18",
};

const button = (key, title) => el("button.btn.fm-btn", { type: "button", "data-fm": key, title, "aria-label": title }, [svg(ICON[key], 18)]);

export function toolBar(o, act) {
  const search = el("input.fm-search", { type: "search", placeholder: "Find a show or an address", "aria-label": "Find on the map", value: o.text || "" });
  const problems = el("button.btn.fm-tog", { type: "button", "data-fm": "problems", "aria-pressed": String(!!o.problems), text: "Problems only" });
  const group = el("select.fm-select", { "aria-label": "Input" });
  const family = el("select.fm-select", { "aria-label": "Kind of output" });
  const sums = el("span.fm-sums");
  const node = el("header.fm-bar", {}, [
    el("div.fm-brand", {}, [el("h2", { text: "Flow map" }), sums]),
    el("div.fm-filters", {}, [search, problems, group, family]),
    el("div.fm-acts", {}, [button("fit", "Fit the whole map (F)"), button("full", "Full screen"), button("close", "Close the flow map (Escape)")]),
  ]);
  search.addEventListener("input", () => act("text", search.value));
  group.addEventListener("change", () => act("group", group.value));
  family.addEventListener("change", () => act("family", family.value));
  node.addEventListener("click", (e) => {
    const b = e.target.closest("[data-fm]");
    if (b) act(b.dataset.fm);
  });
  return {
    node, search,
    /** `t` is {shows, live, inputs, outputs, inK, outK, groups, families}. */
    draw(opts, t) {
      problems.setAttribute("aria-pressed", String(!!opts.problems));
      sums.textContent = [`${t.live} of ${t.shows} ${t.shows === 1 ? "show" : "shows"} live`, `${t.inputs} in`, `${t.outputs} out`, t.inK ? `${rate(t.inK)} in` : "", t.outK ? `${rate(t.outK)} out` : ""].filter(Boolean).join(" · ");
      options(group, [["", "Every input"], ...t.groups.map((k) => [k, k])], opts.group);
      options(family, [["", "Every output"], ...FAMILIES.filter(([k]) => t.families.includes(k))], opts.family);
      group.hidden = t.groups.length < 2;
      family.hidden = t.families.length < 2;
    },
  };
}

function options(select, pairs, value) {
  const key = pairs.map((p) => p[0]).join("|");
  if (select.dataset.keys !== key) {
    select.dataset.keys = key;
    select.replaceChildren(...pairs.map(([v, text]) => el("option", { value: v, text })));
  }
  select.value = value || "";
}

export function legend() {
  const item = (cls, text) => el("span", {}, [el(`i.${cls}`), text]);
  return el("div.fm-legend", { "aria-hidden": "true" }, [
    item("t-ok", "flowing"), item("t-live", "on air"), item("t-warn", "errors or waiting"), item("t-bad", "fault"), item("t-off", "no signal"),
    el("span.fm-hint", { text: "Dots run faster and closer with more bit rate. Drag or scroll to move, Ctrl and the wheel or a pinch to zoom." }),
  ]);
}
