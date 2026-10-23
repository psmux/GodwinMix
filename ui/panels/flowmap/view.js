// The flow map: one screen of everything on a station, what comes in, what
// each show does with it and what goes out, with dots that run at each
// link's bit rate. Opened from View > Flow map or the headend view's bar;
// it asks for nothing until then and stops asking when it closes
// (docs/explanation/nothing-runs-unless-asked.md).
//
// show.list and show.stats for every show once a second (data.js),
// gpu.status and machine.status once a second (headend/machine.js), and
// each running mixed show's status every two seconds (mixed.js).

import { el } from "../../shell/dom.js";
import { MachineWatch } from "../headend/machine.js";
import { drawer } from "../headend/drawer.js";
import { sheet } from "../headend/sheet.js";
import { FlowData } from "./data.js";
import { MixedWatch } from "./mixed.js";
import { FlowMap } from "./map.js";
import { build } from "./graph.js";
import { kept, pathOf } from "./path.js";
import { pointer } from "./pointer.js";
import { tip } from "./tip.js";
import { legend, toolBar } from "./tools.js";
import { totals, keyed, pick, watchTheme } from "./act.js";

let open = null;

export function toggleFlowMap(client) {
  if (open) return open.close();
  open = openFlowMap(client);
  return open;
}

export function showFlowMap(client) {
  return open || toggleFlowMap(client);
}

function styled() {
  const link = el("link#gmx-flowmap", { rel: "stylesheet", href: new URL("./flowmap.css", import.meta.url).href });
  const done = new Promise((r) => { link.onload = r; link.onerror = r; });
  if (!document.getElementById("gmx-flowmap")) document.head.append(link);
  else return Promise.resolve();
  return done;
}

export function openFlowMap(client) {
  const ready = Promise.all([styled(), sheet("headend"), sheet("tiles")]);
  const opts = { text: "", problems: false, group: "", family: "" };
  const view = { client, opts, selected: null, hover: null, fitted: false };
  const canvas = el("canvas.fm-canvas", { role: "img", "aria-label": "Live map of what comes into this station, what each show does with it and what goes out, with moving dots for the bit rate on each link" });
  const empty = el("p.fm-empty", { hidden: true });
  const stage = el("div.fm-stage", {}, [canvas, empty, legend()]);
  const tools = toolBar(opts, (what, value) => act(what, value));
  const main = el("div.fm-main", {}, [stage]);
  const root = el("section.fm", { role: "region", "aria-label": "Flow map" }, [tools.node, main]);
  const bar = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${bar ? Math.round(bar.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);

  const map = new FlowMap(canvas);
  // For a test or a screenshot to find the map from the page.
  canvas.flow = view;
  const card = tip(stage);
  let frame = 0;
  view.later = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => view.draw()); };
  const data = new FlowData(client, view.later);
  const box = new MachineWatch(client, view.later);
  const mixed = new MixedWatch(client, view.later);
  const side = drawer({ get client() { return client; }, data, later: view.later, set selected(v) { view.selected = v; }, get selected() { return view.selected; } });
  main.append(side.node);
  const ctx = () => ({ graph: map.graph, find: (id) => data.find(id), stats: data.stats, mixed: mixed.detail, gpu: box.gpu });

  /** The look follows the hover and the filter; both are kept by id across snapshots. */
  function look() {
    const lit = view.hover ? pathOf(map.graph, view.hover) : null;
    const keep = kept(map.graph, opts);
    map.look = { lit, hover: view.hover, selected: view.selected && `show:${view.selected}`, dims: (id) => !!keep && !keep.has(id) };
    map.dirty = true;
  }

  Object.assign(view, {
    root, map, data, box, mixed, side, tools, card,
    draw() {
      if (!view.ready) return;
      const narrow = stage.clientWidth < 760;
      root.classList.toggle("narrow", narrow);
      const shows = data.missing ? [data.page(client)] : data.shows;
      mixed.follow(shows, data.missing ? "" : data.current);
      const graph = build({ shows, stats: data.stats, series: data.series, mixed: mixed.detail, gpu: box.gpu, missing: false });
      map.set(graph, narrow);
      map.cap = narrow ? 33 : 0;
      if (!view.fitted && graph.nodes.size) {
        // The whole map when it reads at that size, else its width from the top.
        view.fitted = true;
        map.fit(true);
        if (map.v.s < 0.55) map.fit(false);
      }
      look();
      if (card.id) card.show({ kind: graph.nodes.has(card.id) ? "node" : "link", id: card.id }, view.tx || 0, view.ty || 0, ctx());
      tools.draw(opts, totals(graph, shows));
      if (side.id) side.draw();
      const text = !data.loaded ? "Reading what this station runs" : !graph.nodes.size ? "Nothing runs on this station yet. Add a source or a show and it appears here." : "";
      empty.hidden = !text;
      empty.textContent = text;
    },
    close() {
      for (const w of [data, box, mixed, map]) w.stop();
      side.close();
      resize.disconnect();
      theme.disconnect();
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("visibilitychange", wake);
      if (document.fullscreenElement === root) document.exitFullscreen().catch(() => {});
      root.remove();
      open = null;
    },
  });

  function act(what, value) {
    if (what === "close") return view.close();
    if (what === "fit") return map.fit(true);
    if (what === "full") return document.fullscreenElement ? document.exitFullscreen().catch(() => {}) : root.requestFullscreen && root.requestFullscreen().catch(() => {});
    if (what === "problems") opts.problems = !opts.problems;
    else opts[what] = value;
    look();
    tools.draw(opts, totals(map.graph, data.missing ? [data.page(client)] : data.shows));
  }

  pointer(canvas, map, {
    hover(hit, p) {
      const id = hit ? hit.id : null;
      if (p) [view.tx, view.ty] = p;
      canvas.classList.toggle("over", !!hit && hit.kind === "node");
      if (id !== view.hover) (view.hover = id), look();
      if (hit && p) card.show(hit, p[0], p[1], ctx());
      else card.hide();
    },
    pick: (hit) => pick(view, hit),
  });
  const onKey = (e) => keyed(view, e);
  window.addEventListener("keydown", onKey, true);
  const wake = () => (document.hidden ? map.stop() : (map.start(), view.later()));
  document.addEventListener("visibilitychange", wake);
  const resize = new ResizeObserver(() => { map.resize(stage.clientWidth, stage.clientHeight); view.later(); });
  resize.observe(stage);
  const theme = watchTheme(() => map.colours());
  data.start();
  box.start();
  mixed.start();
  ready.then(() => { view.ready = true; map.colours(); map.resize(stage.clientWidth, stage.clientHeight); map.start(); view.later(); });
  return view;
}
