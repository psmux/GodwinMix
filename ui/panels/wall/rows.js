// A show as a row of the table and as a tile of the grid, the table's
// header with its sort buttons, and a group's band.

import { el } from "../../shell/dom.js";
import { alarms, inputCell, loadCell, mixSwitch, outputChip, rateCell } from "./cells.js";
import { healthOf, mixed, pictured } from "./model.js";
import { channelTile } from "./channel-rows.js";

export const COLUMNS = [
  ["pic", ""], ["name", "Show"], ["input", "Input"], ["kbps", "Bitrate"],
  ["outputs", "Outputs"], ["load", "Load"], ["alarms", "Alarms"], ["mixing", "Mixing"],
];

const STATE_WORD = { running: "running", live: "running", starting: "starting", stopped: "stopped", failed: "failed" };

export function header(sort, dir) {
  return el("div.wl-head", { role: "row" }, COLUMNS.map(([id, title]) => {
    if (!title) return el("span.wl-hcell", { role: "columnheader" });
    const on = sort === id;
    return el("span.wl-hcell", { role: "columnheader", "aria-sort": on ? (dir === 1 ? "ascending" : "descending") : "none" }, [
      el("button.wl-sort", { type: "button", "data-sort": id, title: `Sort by ${title.toLowerCase()}` }, [title, on ? el("span.wl-arrow", { text: dir === 1 ? "↓" : "↑" }) : null]),
    ]);
  }));
}

export function band(item) {
  return el(`div.wl-band.${item.key}`, { role: "row" }, [el("span", { role: "gridcell", text: item.label }), el("span.wl-count", { text: String(item.count) })]);
}

/** The name and the state under it, the same in a row and a tile. */
function title(show) {
  const h = healthOf(show);
  const word = STATE_WORD[show.state] || show.state || "";
  return el("span.wl-title", {}, [
    el("span.wl-name", {}, [el(`span.wl-health.${h.state}`, { "aria-label": h.state }), el("span.wl-nametext", { text: show.name })]),
    el("span.wl-sub", { text: [word, show.on_air ? `on air: ${show.on_air}` : "", show.input && show.input.program ? `TS program ${show.input.program}` : ""].filter(Boolean).join(" · ") }),
  ]);
}

function outputs(show, st) {
  const by = new Map(((st && st.outputs) || []).map((o) => [o.id, o]));
  const list = show.outputs || [];
  if (!list.length) return el("span.wl-calm", { text: mixed(show) ? "Outputs inside the show" : "No outputs" });
  return el("span.wl-chips", {}, list.map((o) => outputChip(o, by.get(o.id))));
}

function frame(show, ctx) {
  const black = healthOf(show).alarms.some((a) => a.kind === "black") ? el("span.wl-black", { text: "black" }) : null;
  // A show that is not running says so where its picture would be.
  const state = pictured(show) ? null : el("span.wl-picstate", { text: STATE_WORD[show.state] || show.state });
  return el("span.wl-frame", { "data-id": show.id }, [ctx.thumbs.img(show.id), black, state]);
}

const cls = (show, ctx) => `${healthOf(show).state}${ctx.cursor === show.id ? ".cursor" : ""}`;

export function row(show, ctx) {
  const st = ctx.data.stats.get(show.id);
  const cell = (child, extra = "") => el(`span.wl-cell${extra}`, { role: "gridcell" }, [child]);
  return el(`div.wl-row.${cls(show, ctx)}`, { role: "row", id: `wl-${show.id}`, "data-id": show.id, "aria-selected": String(ctx.cursor === show.id) }, [
    cell(frame(show, ctx), ".pic"),
    cell(title(show)),
    cell(inputCell(show, st)),
    cell(rateCell(st, ctx.data.history.get(show.id))),
    cell(outputs(show, st), ".outs"),
    cell(loadCell(show, st)),
    cell(alarms(show, ctx.acked, ctx.now), ".al"),
    cell(mixSwitch(show)),
  ]);
}

export function tile(show, ctx) {
  const st = ctx.data.stats.get(show.id);
  return el(`div.wl-tile.${cls(show, ctx)}`, { role: "gridcell", id: `wl-${show.id}`, "data-id": show.id, "aria-selected": String(ctx.cursor === show.id) }, [
    frame(show, ctx),
    el("div.wl-tilehead", {}, [title(show), mixSwitch(show)]),
    el("div.wl-tilein", {}, [inputCell(show, st), rateCell(st, ctx.data.history.get(show.id))]),
    outputs(show, st),
    alarms(show, ctx.acked, ctx.now),
  ]);
}

/** One line of tiles: what the virtual list draws in tile view. */
export function tileLine(item, ctx) {
  if (item.kind === "group") return band(item);
  return el("div.wl-tiles", { role: "row", style: { gridTemplateColumns: `repeat(${ctx.cols}, minmax(0, 1fr))` } }, item.shows.map((s) => (s.kind === "channel" ? channelTile(s, ctx) : tile(s, ctx))));
}

/** Rows into lines of `cols` tiles, a band on its own line. A channel
 * stream goes into a line as itself, since it is not a show. */
export function lines(items, cols) {
  const out = [];
  let line = null;
  for (const it of items) {
    if (it.kind === "group") { out.push(it); line = null; continue; }
    if (!line || line.shows.length === cols) out.push((line = { kind: "line", shows: [] }));
    line.shows.push(it.kind === "channel" ? it : it.show);
  }
  return out;
}
