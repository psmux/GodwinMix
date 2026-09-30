// The routing grid: inputs down the side, outputs across, a table so the
// headers can stay put while it scrolls both ways. Every group of outputs
// ends in a + column for adding one more to that channel or show.

import { el } from "../../shell/dom.js";
import { cell, costWords } from "./model.js";

const STATE = { live: "live", connecting: "connecting", reconnecting: "connecting", waiting: "idle", failed: "failed", error: "failed", off: "idle", stopped: "idle" };
const dot = (state) => el(`span.dot.${STATE[state] || "idle"}`, { "aria-hidden": "true" });

/** @param {Array} list groups, already filtered */
export function grid(list, plans, collapsed) {
  const shown = list.filter((g) => g.cols.length || g.kind === "show" || g.kind === "channel");
  const table = el("table.rt-grid", { "aria-label": "Every input and where it goes" });
  table.append(head(shown));
  for (const g of shown) table.append(body(g, shown, plans, collapsed.has(g.key)));
  return table;
}

function head(list) {
  const top = el("tr.rt-groups", {}, [el("th.rt-corner", { rowSpan: 2, scope: "col" }, [el("span.rt-dim", { text: "Inputs ↓  Outputs →" })])]);
  const names = el("tr.rt-outs");
  for (const g of list) {
    top.append(el("th.rt-ghead", { colSpan: g.cols.length + 1, scope: "colgroup", "data-group": g.key }, [el("span.rt-kind", { text: g.kind === "show" ? "Show" : "Channel" }), el("span", { text: g.name })]));
    for (const c of g.cols) {
      names.append(el("th.rt-out", { scope: "col", title: `${c.label}${c.sub ? `, ${c.sub}` : ""}` }, [el("span.rt-name", {}, [dot(c.state), el("span", { text: c.label })]), el("span.rt-sub", { text: c.sub })]));
    }
    names.append(el("th.rt-out.rt-addcol", { scope: "col" }, [el("button.rt-add", { type: "button", "data-add": g.key, title: `Add a destination to ${g.name}`, "aria-label": `Add a destination to ${g.name}`, text: "+" })]));
  }
  return el("thead", {}, [top, names]);
}

function body(g, list, plans, closed) {
  const width = list.reduce((n, x) => n + x.cols.length + 1, 1);
  const tb = el("tbody.rt-group", { "data-group": g.key });
  tb.append(el("tr.rt-band", {}, [el("th", { colSpan: width, scope: "rowgroup" }, [band(g, closed)])]));
  if (closed) return tb;
  for (const r of g.rows) {
    const tr = el("tr", {}, [el("th.rt-in", { scope: "row" }, [el("span.rt-name", {}, [r.live ? el("span.dot.onair", { "aria-label": "live" }) : null, el("span", { text: r.label })]), el("span.rt-sub", { text: r.sub || "" })])]);
    for (const cg of list) {
      for (const c of cg.cols) tr.append(td(cell(g, r, cg, c, plans), g, r, cg, c));
      tr.append(el(`td.rt-addcell${cg.key === g.key ? ".own" : ""}`));
    }
    tb.append(tr);
  }
  return tb;
}

function band(g, closed) {
  const parts = [
    el("button.rt-fold", { type: "button", "data-fold": g.key, "aria-expanded": String(!closed), title: closed ? "Show its inputs" : "Hide its inputs" }, [el("span.rt-caret", { text: closed ? "▸" : "▾" }), el("span.rt-kind", { text: g.kind === "show" ? "Show" : "Channel" }), el("strong", { text: g.name })]),
    el("span.rt-dim", { text: `${g.rows.length} input${g.rows.length === 1 ? "" : "s"}, ${g.cols.length} output${g.cols.length === 1 ? "" : "s"}` }),
  ];
  if (g.kind === "show" && g.state !== "running") {
    parts.push(el(`span.rt-state.${g.state}`, { text: g.state === "failed" ? `failed${g.show.error ? `: ${g.show.error}` : ""}` : g.state }));
    if (g.state !== "starting") parts.push(el("button.btn.sm", { type: "button", "data-start": g.key, text: "Start" }));
  } else if (g.kind === "show" && !g.loaded) parts.push(el("span.rt-dim", { text: "Reading its outputs…" }));
  return el("div.rt-bandin", {}, parts);
}

function td(c, g, r, cg, col) {
  const own = g.key === cg.key ? ".own" : "";
  const at = { "data-group": cg.key, "data-row": r.key, "data-col": col.key };
  if (c.kind === "route") {
    const cost = costWords(c.cost);
    return el(`td.rt-cell${own}`, {}, [el(`button.rt-route.${c.tone}`, { type: "button", ...at, "data-act": "edit", title: `${r.label} to ${col.label}: ${c.format}, ${c.where}, ${cost}. Press to change it.` }, [
      el("span.rt-format", { text: c.format }),
      el("span.rt-where", { text: c.where }),
      el("span.rt-cost", { text: cost }),
    ])]);
  }
  if (c.kind === "offer") return el(`td.rt-cell${own}`, {}, [el("button.rt-offer", { type: "button", ...at, "data-act": "send", title: `Send ${r.label} to ${col.label}`, "aria-label": `Send ${r.label} to ${col.label}`, text: "+" })]);
  if (c.kind === "via") return el(`td.rt-cell${own}`, {}, [el("span.rt-via", { title: `${r.label} is in the programme, so ${col.label} carries it` }, [el("span.rt-pip"), "in programme"])]);
  return el(`td.rt-cell${own}`);
}
