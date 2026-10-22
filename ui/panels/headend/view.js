// The headend view: a box running many channels, as a control room sees
// it. The box's totals in a band, the GPU strip, the shared inputs, then a
// tile per channel with its picture, state, rates and sound, and a drawer
// with one channel's detail. Opened from View > Headend or the wall; it
// asks for nothing until then, and stops asking when it closes.
//
// show.stats for every channel once a second (data.js), gpu.status and
// machine.status once a second (machine.js), pictures for the tiles on
// screen only, every two seconds (../wall/thumbs.js).

import { el } from "../../shell/dom.js";
import { sheet } from "./sheet.js";
import { Thumbs } from "../wall/thumbs.js";
import { pictured } from "../wall/model.js";
import { HeadendData } from "./data.js";
import { MachineWatch } from "./machine.js";
import { channels, counts, groups } from "./model.js";
import { band } from "./band.js";
import { gpuStrip } from "./gpu.js";
import { groupStrip } from "./groups.js";
import { toolBar } from "./tools.js";
import { Grid } from "./grid.js";
import { drawer } from "./drawer.js";
import { act, keyed, prefs, savePrefs } from "./act.js";
import { totals } from "./totals.js";

let open = null;

export function toggleHeadend(client) {
  if (open) return open.close();
  open = openHeadend(client);
  return open;
}

export function showHeadend(client) {
  return open || toggleHeadend(client);
}

export function openHeadend(client) {
  const styled = Promise.all([sheet("headend"), sheet("tiles")]);
  const opts = prefs();
  const view = { client, opts, selected: null };
  const top = band((what) => act(view, what));
  const gpu = gpuStrip();
  const grps = groupStrip();
  const tools = toolBar(opts, (what, value) => act(view, what, value));
  const body = el("div.he-body");
  const empty = el("p.he-empty", { hidden: true });
  const scroll = el("div.he-scroll", { tabindex: "0", "aria-label": "Channels" }, [body, empty]);
  const strips = el("div.he-strips", {}, [gpu.node, grps.node]);
  const root = el("section.he", { role: "region", "aria-label": "Headend" }, [top.node, strips, tools.node, el("div.he-main", {}, [scroll])]);
  const bar = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${bar ? Math.round(bar.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);

  let frame = 0;
  view.later = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => view.draw()); };
  const data = new HeadendData(client, view.later);
  const box = new MachineWatch(client, view.later);
  const thumbs = new Thumbs(client, 320);
  const grid = new Grid(scroll, body, thumbs);
  const side = drawer(view);
  root.querySelector(".he-main").append(side.node);

  Object.assign(view, {
    root, scroll, data, box, thumbs, grid, side, tools,
    draw() {
      if (!view.styled) return;
      const narrow = root.clientWidth < 760;
      root.classList.toggle("narrow", narrow);
      const { rows, all } = channels(data.shows, data.stats, data.series, opts);
      const g = groups(data.shows, data.stats, data.series);
      top.draw(totals(all, data, box));
      gpu.draw(box);
      grps.draw(g, data.stats, opts.group);
      tools.draw(opts, counts(all), g.shared.map((x) => x.key));
      grid.layout(narrow ? 150 : 236);
      grid.set(rows);
      view.fetch();
      if (side.id) side.draw();
      const text = !data.loaded ? "Reading the channels on this station" : data.missing ? "This station keeps one show, so there are no channels to watch here." : !data.shows.length ? "No channels yet. Add shows on the monitoring wall makes them." : !rows.length ? "No channel matches that filter." : "";
      empty.hidden = !text;
      empty.textContent = text;
    },
    /** Patch the tiles on screen and ask for their pictures, and no others. */
    fetch(patch = true) {
      const shown = grid.draw((id) => data.series.get(id), view.selected, patch);
      const pics = shown.filter((r) => pictured(r.show) && r.state !== "offline");
      thumbs.blank(shown.filter((r) => !pics.includes(r)).map((r) => r.show.id));
      thumbs.visible(pics.map((r) => r.show.id));
    },
    pick(id) {
      view.selected = id;
      side.open(id);
      view.later();
    },
    close() {
      data.stop();
      box.stop();
      thumbs.stop();
      side.close();
      resize.disconnect();
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("visibilitychange", wake);
      if (document.fullscreenElement === root) document.exitFullscreen().catch(() => {});
      root.remove();
      open = null;
      savePrefs(opts);
    },
  });

  scroll.addEventListener("scroll", () => { cancelAnimationFrame(view.sf); view.sf = requestAnimationFrame(() => view.fetch(false)); }, { passive: true });
  root.addEventListener("click", (e) => {
    const grp = e.target.closest("[data-group]");
    if (grp) return act(view, "group", opts.group === grp.dataset.group ? "" : grp.dataset.group);
    const tile = e.target.closest(".he-tile");
    if (tile) view.pick(tile.dataset.id);
  });
  const onKey = (e) => keyed(view, e);
  window.addEventListener("keydown", onKey, true);
  const wake = () => { if (!document.hidden) view.later(); };
  document.addEventListener("visibilitychange", wake);
  const resize = new ResizeObserver(() => view.later());
  resize.observe(root);
  data.start();
  box.start();
  styled.then(() => { view.styled = true; view.later(); });
  return view;
}
