// View > Routing: one screen with every input on this machine and where it
// goes, over the page and under the top bar, so the tabs and the live
// controls stay in reach. It fetches nothing until opened and, once open,
// only the groups on screen (data.js).

import { el } from "../../shell/dom.js";
import { RoutingData } from "./data.js";
import { grid } from "./grid.js";
import { list } from "./list.js";
import { filtered } from "./model.js";
import * as act from "./act.js";

let open = null;

export function toggleRouting(client) {
  if (open) return open.close();
  open = openRouting(client);
  return open;
}

function sheet() {
  if (document.getElementById("gmx-routing-css")) return;
  document.head.append(el("link#gmx-routing-css", { rel: "stylesheet", href: new URL("./routing.css", import.meta.url).href }));
}

export function openRouting(client) {
  sheet();
  const back = document.activeElement;
  const filter = el("input.rt-filter", { type: "search", placeholder: "Filter inputs and outputs", "aria-label": "Filter inputs and outputs" });
  const summary = el("span.rt-dim");
  const scroll = el("div.rt-scroll");
  const root = el("section.routing", { role: "region", "aria-label": "Routing" }, [
    el("header.rt-top", {}, [
      el("h2", { text: "Routing" }), summary, el("span.grow"), legend(), filter,
      el("button.btn.icon", { type: "button", text: "×", title: "Close routing (Escape)", "aria-label": "Close routing", onclick: () => view.close() }),
    ]),
    scroll,
  ]);
  const header = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${header ? Math.round(header.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);

  const collapsed = new Set();
  const seen = new Map();
  let shown = new Set();
  let frame = 0;
  const data = new RoutingData(client, () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(draw); });
  const observer = new IntersectionObserver((entries) => {
    for (const e of entries) seen.set(e.target, e.isIntersecting);
    const now = new Set([...seen].filter(([n, on]) => on && n.isConnected).map(([n]) => n.dataset.group));
    for (const g of data.groups()) {
      if (now.has(g.key) !== shown.has(g.key)) data.visible(g, now.has(g.key));
    }
    shown = now;
  }, { root: scroll });

  function draw() {
    const all = data.groups();
    const groups = filtered(all, filter.value);
    const count = (kind, word) => { const n = all.filter((g) => g.kind === kind).length; return `${n} ${word}${n === 1 ? "" : "s"}`; };
    summary.textContent = `${count("channel", "channel")}, ${count("show", "show")}`;
    const focused = document.activeElement && root.contains(document.activeElement) ? selector(document.activeElement) : null;
    observer.disconnect();
    seen.clear();
    const narrow = scroll.clientWidth < 700;
    // Replacing the table empties the box for a moment, which would put the
    // scroll back at the top on every answer that arrives.
    const [top, left] = [scroll.scrollTop, scroll.scrollLeft];
    scroll.replaceChildren(groups.length ? (narrow ? list : grid)(groups, data.plans, collapsed) : el("p.rt-empty", { text: filter.value ? "Nothing matches that filter." : "Nothing here yet: no channel and no show." }));
    scroll.scrollTop = top;
    scroll.scrollLeft = left;
    for (const n of scroll.querySelectorAll("[data-group]:not(button):not(td)")) observer.observe(n);
    if (focused) scroll.querySelector(focused)?.focus();
  }

  scroll.addEventListener("click", (e) => press(e, data, collapsed, draw));
  filter.addEventListener("input", draw);
  const onKey = (e) => {
    if (e.key !== "Escape" || document.querySelector(".scrim")) return;
    if (e.target === filter && filter.value) return;
    view.close();
  };
  window.addEventListener("keydown", onKey);
  const resize = new ResizeObserver(() => draw());
  resize.observe(scroll);

  const view = {
    root,
    data,
    close() {
      data.stop();
      observer.disconnect();
      resize.disconnect();
      window.removeEventListener("keydown", onKey);
      root.remove();
      open = null;
      if (back && back.focus) back.focus();
    },
  };
  data.start();
  // Not on a phone, where focus in a box brings the keyboard up over the view.
  if (window.innerWidth >= 700) filter.focus();
  return view;
}

/** Enough to find the same button after a redraw. */
function selector(node) {
  const keys = ["data-fold", "data-add", "data-start"].find((k) => node.hasAttribute(k));
  if (keys) return `[${keys}="${CSS.escape(node.getAttribute(keys))}"]`;
  if (node.dataset.col) return `[data-row="${CSS.escape(node.dataset.row || "")}"][data-col="${CSS.escape(node.dataset.col)}"]`;
  return null;
}

function press(e, data, collapsed, draw) {
  const b = e.target.closest("button");
  if (!b) return;
  const groups = data.groups();
  const byKey = (k) => groups.find((g) => g.key === k);
  if (b.dataset.fold) {
    collapsed.has(b.dataset.fold) ? collapsed.delete(b.dataset.fold) : collapsed.add(b.dataset.fold);
    return draw();
  }
  if (b.dataset.start) return act.startShow(data, byKey(b.dataset.start));
  if (b.dataset.add) return act.addTo(data, byKey(b.dataset.add));
  const g = byKey(b.dataset.group);
  const col = g && g.cols.find((c) => c.key === b.dataset.col);
  if (!col) return;
  if (b.dataset.act === "edit") return act.edit(data, g, col);
  if (b.dataset.act === "send") return act.sendHere(data, g, g.rows.find((r) => r.key === b.dataset.row), col);
}

function legend() {
  const item = (tone, text) => el("span.rt-key", {}, [el(`span.rt-swatch.${tone}`), text]);
  return el("span.rt-legend", { "aria-hidden": "true" }, [item("copy", "Copy"), item("prog", "Programme encode"), item("gpu", "GPU encode"), item("cpu", "CPU encode")]);
}
