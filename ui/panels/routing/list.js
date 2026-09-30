// The routing view on a phone: the same routes as the grid, one card per
// output saying what it reads and how, because twenty columns do not fit
// in a hand. An output that could read another stream offers it as a chip.

import { el } from "../../shell/dom.js";
import { cell, costWords } from "./model.js";

export function list(groups, plans, collapsed) {
  const root = el("div.rt-list");
  for (const g of groups) {
    const closed = collapsed.has(g.key);
    const sec = el("section.rt-lgroup", { "data-group": g.key }, [
      el("button.rt-fold.rt-lhead", { type: "button", "data-fold": g.key, "aria-expanded": String(!closed) }, [
        el("span.rt-caret", { text: closed ? "▸" : "▾" }),
        el("span.rt-kind", { text: g.kind === "show" ? "Show" : "Channel" }),
        el("strong", { text: g.name }),
        el("span.rt-dim", { text: g.kind === "show" && g.state !== "running" ? g.state : `${g.cols.length} output${g.cols.length === 1 ? "" : "s"}` }),
      ]),
    ]);
    if (!closed) {
      for (const c of g.cols) sec.append(card(g, c, plans));
      if (g.kind === "show" && g.state !== "running" && g.state !== "starting") sec.append(el("button.btn", { type: "button", "data-start": g.key, text: `Start ${g.name}` }));
      else sec.append(el("button.rt-ladd", { type: "button", "data-add": g.key, text: g.cols.length ? "+ Add a destination" : "No outputs yet. + Add a destination" }));
    }
    root.append(sec);
  }
  return root;
}

function card(g, col, plans) {
  const routes = g.rows.map((r) => [r, cell(g, r, g, col, plans)]);
  const [row, route] = routes.find(([, c]) => c.kind === "route") || [null, null];
  const at = { "data-group": g.key, "data-col": col.key };
  const head = el("button.rt-lcard-main", { type: "button", ...at, "data-row": row ? row.key : "", "data-act": "edit" }, [
    el("span.rt-name", {}, [el(`span.dot.${col.state === "live" ? "live" : col.state === "failed" ? "failed" : "idle"}`), el("strong", { text: col.label }), el("span.rt-dim", { text: col.sub })]),
    el("span", { text: row ? `from ${row.label}` : "Waiting for an input" }),
    route ? el(`span.rt-lformat.${route.tone}`, { text: `${route.format} · ${route.where} · ${costWords(route.cost)}` }) : null,
  ]);
  const offers = routes.filter(([, c]) => c.kind === "offer").map(([r]) => el("button.rt-chip", { type: "button", ...at, "data-row": r.key, "data-act": "send", text: `Send ${r.label} instead` }));
  return el("div.rt-lcard", {}, [head, offers.length ? el("div.rt-chips", {}, offers) : null]);
}
