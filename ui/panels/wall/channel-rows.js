// A channel stream as a row of the wall's table and as a tile: its picture,
// the channel and stream names, how it arrives and its shape, its bit rate,
// and how many of its destinations are sending, in red while one fails.
// A click opens the Channels panel (act.js).

import { el } from "../../shell/dom.js";
import { kbps } from "./model.js";
import { channelHealth, channelPictured, sendingText, streamFormat } from "./channel-model.js";

const WORD = { live: "live", idle: "idle", waiting: "waiting for an encoder", off: "switched off" };
const PROTO = { rtmp: "RTMP", rtmps: "RTMPS", srt: "SRT", whip: "WHIP" };

function frame(it, ctx) {
  const state = channelPictured(it) ? null : el("span.wl-picstate", { text: it.stream ? WORD[it.state] || it.state : "no stream" });
  return el("span.wl-frame", { "data-channel": it.key }, [ctx.chanThumbs.img(it.key), state]);
}

function title(it) {
  const h = channelHealth(it);
  const sub = [it.stream ? `${it.stream} · ${WORD[it.state] || it.state}` : WORD[it.state], it.source ? `source ${it.source}` : ""].filter(Boolean).join(" · ");
  return el("span.wl-title", {}, [
    el("span.wl-name", {}, [el(`span.wl-health.${h}`, { "aria-label": h }), el("span.wl-nametext", { text: it.title })]),
    el("span.wl-sub", { text: sub }),
  ]);
}

function input(it) {
  const line = [it.stream ? PROTO[it.protocol] || it.protocol : "", streamFormat(it)].filter(Boolean).join(" · ");
  return el("span.wl-in", {}, [el("span.wl-line", { text: line || (it.state === "off" ? "Off" : "Waiting for an encoder") }), el("span.wl-sub", { text: `Channel ${it.channel}` })]);
}

/** "3 of 4 sending", red while a destination fails, with why in its title. */
export function sendingChip(it) {
  const state = it.failing ? "failed" : it.sending ? "live" : "idle";
  const title = it.problems.length ? it.problems.join("\n") : sendingText(it);
  return el(`span.wl-chip.wl-sending.${state}`, { title }, [el("span.wl-dot"), el("span.wl-chipname", { text: sendingText(it) })]);
}

function alarms(it) {
  if (!it.failing) return el("span.wl-calm", { text: it.state === "off" ? "Off" : "No alarms" });
  return el("span.wl-alarms", {}, it.problems.map((p) => el("span.wl-alarm.flash", { title: p }, [el("span", { text: p })])));
}

const cls = (it) => `wl-chan.${channelHealth(it)}`;

export function channelRow(it, ctx) {
  const cell = (child, extra = "") => el(`span.wl-cell${extra}`, { role: "gridcell" }, [child]);
  return el(`div.wl-row.${cls(it)}`, { role: "row", "data-channel": it.key }, [
    cell(frame(it, ctx), ".pic"),
    cell(title(it)),
    cell(input(it)),
    cell(el("span.wl-num", { text: it.state === "live" ? kbps(it.kbps) : "" })),
    cell(sendingChip(it), ".outs"),
    cell(el("span")),
    cell(alarms(it), ".al"),
    cell(el("span")),
  ]);
}

export function channelTile(it, ctx) {
  return el(`div.wl-tile.${cls(it)}`, { role: "gridcell", "data-channel": it.key }, [
    frame(it, ctx),
    el("div.wl-tilehead", {}, [title(it)]),
    el("div.wl-tilein", {}, [input(it), el("span.wl-num", { text: it.state === "live" ? kbps(it.kbps) : "" })]),
    sendingChip(it),
    alarms(it),
  ]);
}
