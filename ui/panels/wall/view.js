// The monitoring wall: every show on this station, as rows or tiles, over
// the page and under the top bar. Opened from View, the palette and the
// show tabs; fetches nothing until then. Only the rows on screen are drawn,
// read with show.stats and asked for pictures (data.js, thumbs.js). Under
// the shows, a Channels group has every channel stream with its picture
// (channel-data.js, channel-rows.js).

import { el } from "../../shell/dom.js";
import { sheet } from "./sheet.js";
import { WallData } from "./data.js";
import { Thumbs } from "./thumbs.js";
import { Virtual } from "./virtual.js";
import { header, row, band, tileLine, lines } from "./rows.js";
import { pictured, rows, summary } from "./model.js";
import { topBar } from "./top.js";
import { keyed, press, prefs, savePrefs } from "./act.js";
import { ChannelWatch } from "./channel-data.js";
import { channelGroup, channelPictured } from "./channel-model.js";
import { channelRow } from "./channel-rows.js";
import { channelThumbUrl } from "../channels/picture.js";

let open = null;

export function toggleWall(client) {
  if (open) return open.close();
  open = openWall(client);
  return open;
}

/** Open, or bring forward when already open. */
export function showWall(client) {
  return open || toggleWall(client);
}

const BAND = 34;

export function openWall(client) {
  const styled = sheet("wall");
  const back = document.activeElement;
  const opts = prefs();
  const view = { client, opts, acked: new Set(), cursor: null, items: [], cols: 1 };
  const top = topBar(opts, (what, value) => press(view, what, value));
  const head = el("div.wl-headwrap");
  const body = el("div.wl-body", { role: "rowgroup" });
  const empty = el("p.wl-empty", { hidden: true });
  const scroll = el("div.wl-scroll", { tabindex: "0", role: "grid", "aria-label": "Shows" }, [head, body, empty]);
  const root = el("section.wall", { role: "region", "aria-label": "Monitoring wall" }, [top.node, scroll]);
  const bar = document.querySelector(".slot-header, gmx-header");
  root.style.top = `${bar ? Math.round(bar.getBoundingClientRect().bottom) : 44}px`;
  document.body.append(root);

  let frame = 0;
  const later = () => { cancelAnimationFrame(frame); frame = requestAnimationFrame(() => view.draw()); };
  const data = new WallData(client, later);
  const thumbs = new Thumbs(client);
  const chans = new ChannelWatch(client, later);
  const split = (key) => [key.slice(0, key.indexOf("/")), key.slice(key.indexOf("/") + 1)];
  const chanThumbs = new Thumbs(client, 160, (key, width, t) => channelThumbUrl(client, ...split(key), Math.max(width, 320), t));
  const ctx = () => ({ data, thumbs, chanThumbs, acked: view.acked, now: Date.now(), cursor: view.cursor, cols: view.cols });
  const list = new Virtual(scroll, body, (it) => {
    const c = ctx();
    if (it.kind === "group") return band(it);
    if (it.kind === "channel") return channelRow(it, c);
    return opts.mode === "tiles" ? tileLine(it, c) : row(it.show, c);
  });

  Object.assign(view, {
    root, scroll, data, thumbs, chans, chanThumbs, list, top,
    narrow: () => scroll.clientWidth < 760,
    draw() {
      if (!view.styled) return;
      const tiles = opts.mode === "tiles";
      const narrow = view.narrow();
      root.classList.toggle("narrow", narrow);
      root.classList.toggle("tiles", tiles);
      top.sync(opts);
      top.counts({ ...summary(data.shows, data.stats, data.gov), channelAlarms: new Set(chans.items.filter((it) => it.failing).map((it) => it.channel)).size });
      view.items = [...rows(data.shows, data.stats, opts), ...channelGroup(chans.items, opts)];
      view.cols = tiles ? Math.max(1, Math.floor((scroll.clientWidth - 24) / (narrow ? 170 : 236))) : 1;
      head.replaceChildren(tiles || narrow ? "" : header(opts.sort, opts.dir));
      const tileH = Math.round(((scroll.clientWidth - 24) / view.cols - 12) * 9 / 16) + (narrow ? 150 : 168);
      const items = tiles ? lines(view.items, view.cols) : view.items;
      list.set(items, (it) => (it.kind === "group" ? BAND : tiles ? tileH : narrow ? 84 : 60));
      say(view, empty);
      view.fetch();
    },
    /** Which shows are in view, for stats and pictures. Not the margin. */
    fetch() {
      if (!view.styled) return;
      const [a, b] = list.inView();
      const shown = [];
      const streams = [];
      for (let i = a; i <= b; i++) {
        const it = list.items[i];
        if (!it) continue;
        if (it.kind === "show") shown.push(it.show);
        if (it.kind === "channel") streams.push(it);
        if (it.kind === "line") for (const s of it.shows) (s.kind === "channel" ? streams : shown).push(s);
      }
      // A channel stream's picture is asked for only while it is live and on screen.
      chanThumbs.blank(streams.filter((s) => !channelPictured(s)).map((s) => s.key));
      chanThumbs.visible(streams.filter(channelPictured).map((s) => s.key));
      data.visible(shown.map((s) => s.id));
      thumbs.width = opts.mode === "tiles" ? 320 : 160;
      // A show that is not running has no picture to ask for, and an old one
      // would say it is still on; its frame says its state instead.
      const live = data.noStats ? [] : shown.filter(pictured);
      thumbs.blank(shown.filter((s) => !pictured(s)).map((s) => s.id));
      // A station without show.stats has no pictures of its shows either.
      thumbs.visible(live.map((s) => s.id));
    },
    close() {
      data.stop();
      thumbs.stop();
      chans.stop();
      chanThumbs.stop();
      resize.disconnect();
      window.removeEventListener("keydown", onKey, true);
      document.removeEventListener("visibilitychange", wake);
      root.remove();
      open = null;
      savePrefs(opts);
      if (back && back.focus) back.focus();
    },
  });

  scroll.addEventListener("scroll", () => { cancelAnimationFrame(view.sf); view.sf = requestAnimationFrame(() => { list.draw(); view.fetch(); }); }, { passive: true });
  scroll.addEventListener("click", (e) => press(view, "click", e));
  const onKey = (e) => keyed(view, e);
  window.addEventListener("keydown", onKey, true);
  const wake = () => { if (!document.hidden) view.fetch(); };
  document.addEventListener("visibilitychange", wake);
  const resize = new ResizeObserver(() => later());
  resize.observe(scroll);
  data.start();
  chans.start();
  styled.then(() => { view.styled = true; later(); });
  if (window.innerWidth >= 760) scroll.focus();
  return view;
}

function say(view, empty) {
  const d = view.data;
  // Channels on screen are enough to say nothing about shows.
  const channels = view.items.some((it) => it.kind === "channel");
  const text = channels ? "" : !d.loaded ? "Reading the shows on this station" : d.missing ? "This station keeps one show and has no list of shows to watch." : !d.shows.length ? "No shows yet. Add shows puts in one feed or a few hundred." : !view.items.length ? "Nothing matches that filter." : "";
  empty.hidden = !text;
  empty.textContent = text;
}
