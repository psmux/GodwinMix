// The headend view: a box running many channels, as a control room sees
// it. The box's totals in a band, the GPU strip, the shared inputs, then a
// tile per channel with its picture, state, rates and sound, and a drawer
// with one channel's detail. Opened from View > Headend or the wall; it
// asks for nothing until then, and stops asking when it closes.
//
// show.stats for every channel once a second (data.js), gpu.status and
// machine.status once a second (machine.js), pictures for the tiles on
// screen only, every two seconds (../wall/thumbs.js). headend.channel.list
// once when it opens and after each change made here, so the drawer knows
// which shows are channels the station keeps. With no channels, the empty
// grid offers the three ways to a first one: add, scan or import.

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

/** What an empty headend offers: three ways to its first channel. */
function firstRun(go) {
  const way = (what, title, words) => el("button.he-way", { type: "button", "data-way": what, onclick: () => go(what) }, [el("strong", { text: title }), el("span", { text: words })]);
  return el("div.he-first", { hidden: true }, [
    el("h3.he-firsth", { text: "No channels yet" }),
    el("p.he-firstp", { text: "Set up the channels this headend runs. Each one takes a feed, converts it if you want, and serves it as HLS." }),
    el("div.he-ways", {}, [
      way("add", "Add a channel", "One feed by hand: UDP or RTP multicast, SRT, RTMP, RTSP, HLS or a file."),
      way("scan", "Scan a multicast feed", "List the programs in a group and port, and add the ones you tick."),
      way("import", "Import a Flussonic config", "Each stream becomes a channel the station keeps and you can edit."),
    ]),
  ]);
}

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
  const ways = firstRun((what) => act(view, what));
  const scroll = el("div.he-scroll", { tabindex: "0", "aria-label": "Channels" }, [body, empty, ways]);
  const strips = el("div.he-strips", {}, [gpu.node, grps.node]);
  const root = el("section.he", { role: "region", "aria-label": "Headend" }, [top.node, strips, tools.node, el("div.he-main", {}, [scroll])]);
  const bar = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${bar ? Math.round(bar.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);
  // A show tab closes this page, since it covers the show's own page.
  root.dataset.cover = "";
  root.leave = () => view.close();

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
      const none = data.loaded && !data.missing && !data.shows.length;
      const text = !data.loaded ? "Reading the channels on this station" : data.missing ? "This station keeps one show, so there are no channels to watch here." : none ? "" : !rows.length ? "No channel matches that filter." : "";
      empty.hidden = !text;
      empty.textContent = text;
      ways.hidden = !none;
    },
    /** Patch the tiles on screen and ask for their pictures, and no others. */
    fetch(patch = true) {
      const shown = grid.draw((id) => data.series.get(id), view.selected, patch);
      const pics = shown.filter((r) => pictured(r.show) && r.state !== "offline");
      thumbs.blank(shown.filter((r) => !pics.includes(r)).map((r) => r.show.id));
      thumbs.visible(pics.map((r) => r.show.id));
    },
    /** headend.channel.list again, after a change made on this page. */
    async reload() {
      view.setup = await client.call("headend.channel.list", {}).catch(() => null);
      data.read().catch(() => {});
      view.later();
      return view.setup;
    },
    /** The channel the station keeps for a show, if it keeps one. */
    entry(showId) {
      return ((view.setup && view.setup.channels) || []).find((e) => e.show === showId) || null;
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
  view.reload();
  styled.then(() => { view.styled = true; view.later(); });
  return view;
}
