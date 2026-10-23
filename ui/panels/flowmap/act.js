// What a press, a key or a click does on the flow map, and the totals in
// its bar. A click on a channel opens the headend view's detail drawer
// beside the map; a click on a mixed show opens that show, where its own
// panels are; a click on nothing closes the drawer.

import { toast } from "../../shell/toast.js";

/** The bar's counts, from the graph. */
export function totals(g, shows) {
  const t = { shows: shows.length, live: 0, inputs: 0, outputs: 0, inK: 0, outK: 0, groups: [], families: [] };
  if (!g) return t;
  for (const n of g.nodes.values()) {
    if (n.kind === "show" && n.tone === "ok") t.live += 1;
    if (n.col === 0) t.inputs += 1;
    if (n.kind === "input") t.inK += n.rate || 0, n.members && n.members.length > 1 && t.groups.push(n.title);
    if (n.kind === "output") {
      t.outputs += 1;
      t.outK += n.rate || 0;
      if (!t.families.includes(n.family)) t.families.push(n.family);
    }
  }
  t.groups.sort((a, b) => a.localeCompare(b, undefined, { numeric: true }));
  return t;
}

export function pick(view, hit) {
  const n = hit && hit.kind === "node" ? view.map.graph.nodes.get(hit.id) : null;
  if (!n || !n.show && n.show !== "") return view.side.id && view.side.close();
  const show = view.data.find(n.show);
  if (show && show.compositing === false) {
    view.selected = n.show;
    view.side.open(n.show);
    return view.later();
  }
  if (n.show === view.data.current || view.data.missing) {
    toast({ text: `${(show && show.name) || "This show"} is the show this page is on. Its panels are under the map.` });
    return view.close();
  }
  view.close();
  import("../../shell/show-file.js").then((m) => m.switchShow(view.client, n.show));
}

const typing = (t) => t && t.closest && t.closest("input, select, textarea");

/** Escape closes the drawer, then the map; / finds; F fits; + and - zoom. */
export function keyed(view, e) {
  if (document.querySelector(".scrim") || !view.root.isConnected) return;
  if (e.key === "Escape") {
    if (typing(e.target) && e.target.value) return;
    e.preventDefault();
    return view.side.id ? view.side.close() : view.close();
  }
  if (typing(e.target) || e.metaKey || e.ctrlKey || e.altKey) return;
  const m = view.map;
  const mid = [m.v.w / 2, m.v.h / 2];
  const keys = { "/": () => view.tools.search.focus(), f: () => m.fit(true), "+": () => m.zoomAt(...mid, 1.25), "=": () => m.zoomAt(...mid, 1.25), "-": () => m.zoomAt(...mid, 0.8),
    ArrowUp: () => m.pan(0, 60), ArrowDown: () => m.pan(0, -60), ArrowLeft: () => m.pan(60, 0), ArrowRight: () => m.pan(-60, 0) };
  const run = keys[e.key];
  if (!run) return;
  e.preventDefault();
  run();
}

/** Call `fn` when the theme changes: a new theme sheet, or the page's theme attribute. */
export function watchTheme(fn) {
  let t = 0;
  const later = () => { clearTimeout(t); t = setTimeout(fn, 60); };
  const mo = new MutationObserver(later);
  mo.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme", "class", "style"] });
  const link = document.getElementById("gmx-theme");
  if (link) mo.observe(link, { attributes: true, attributeFilter: ["href"] }), link.addEventListener("load", later);
  const mq = matchMedia("(prefers-color-scheme: dark)");
  mq.addEventListener("change", later);
  return { disconnect: () => (mo.disconnect(), mq.removeEventListener("change", later), link && link.removeEventListener("load", later)) };
}
