// The pieces a row and a tile are made of: a sparkline, an output chip, an
// alarm with its age, the compositing switch. Plain nodes, no state.

import { el } from "../../shell/dom.js";
import { ALARMS, age, codec, format, healthOf, kbps, load, mixed, transport } from "./model.js";

const NS = "http://www.w3.org/2000/svg";

/** The input's bitrate over the last forty seconds, as one line. */
export function sparkline(values, w = 48, h = 18) {
  const s = document.createElementNS(NS, "svg");
  s.setAttribute("viewBox", `0 0 ${w} ${h}`);
  s.setAttribute("width", String(w));
  s.setAttribute("height", String(h));
  s.setAttribute("class", "wl-spark");
  s.setAttribute("aria-hidden", "true");
  if (values && values.length > 1) {
    const max = Math.max(...values, 1);
    const step = w / (values.length - 1);
    const pts = values.map((v, i) => `${(i * step).toFixed(1)},${(h - 1 - (v / max) * (h - 3)).toFixed(1)}`).join(" ");
    const line = document.createElementNS(NS, "polyline");
    line.setAttribute("points", pts);
    s.append(line);
  }
  return s;
}

const OUT_WORD = { live: "live", connecting: "connecting", reconnecting: "reconnecting", failed: "failed", stopped: "off", idle: "idle" };

/** What an output is sent as: "Copy", or its format in a few words. */
export function outFormat(o, st) {
  if (st && st.rendition_text) return st.rendition_text;
  const r = o.rendition;
  if (!r) return "Copy";
  if (r.preset) return r.preset.replace(/-/g, " ");
  if (r.ladder) return `Ladder of ${r.ladder.length}`;
  const v = r.video || {};
  return v.height ? `${v.height}p${v.bitrate_kbps ? ` ${kbps(v.bitrate_kbps)}` : ""}` : "Re-encode";
}

const PLATFORMS = { youtube: "YouTube", facebook: "Facebook", twitch: "Twitch", linkedin: "LinkedIn", kick: "Kick", x: "X" };

export function outName(o) {
  if (o.platform) return PLATFORMS[o.platform] || o.platform.charAt(0).toUpperCase() + o.platform.slice(1);
  const u = String(o.uri || "");
  const m = /^[a-z]+:\/\/@?([^/?]+)/i.exec(u);
  return m ? m[1] : o.id;
}

export function outputChip(o, st) {
  const state = (st && st.state) || o.state || "idle";
  const k = st ? st.kbps : o.kbps;
  const bits = [outName(o), outFormat(o, st), k ? kbps(k) : ""].filter(Boolean);
  const title = `${o.id}: ${o.uri || o.platform || ""}, ${OUT_WORD[state] || state}${(st && st.reconnects) || o.reconnects ? `, ${(st && st.reconnects) || o.reconnects} reconnects` : ""}${o.error ? `. ${o.error}` : ""}`;
  return el(`span.wl-chip.${state}`, { title }, [el("span.wl-dot"), el("span.wl-chipname", { text: bits[0] }), el("span.wl-chipfmt", { text: bits.slice(1).join(" · ") })]);
}

/** An alarm, flashing until acknowledged. The key says which raise of it. */
export const alarmKey = (id, a) => `${id}/${a.kind}/${a.since_ms}`;

export function alarmChip(id, a, acked, now) {
  const warn = a.kind === "cc-errors" || a.kind === "shed" || a.kind === "loss";
  const on = !warn && !acked.has(alarmKey(id, a));
  return el(`span.wl-alarm${warn ? ".warn" : ""}${on ? ".flash" : ""}`, { title: a.detail || ALARMS[a.kind] || a.kind }, [
    el("span", { text: ALARMS[a.kind] || a.kind }),
    el("span.wl-age", { text: age(a.since_ms, now) }),
  ]);
}

export function alarms(show, acked, now) {
  const h = healthOf(show);
  if (!h.alarms.length) return el("span.wl-calm", { text: h.state === "off" ? "Off" : "No alarms" });
  return el("span.wl-alarms", {}, h.alarms.map((a) => alarmChip(show.id, a, acked, now)));
}

/** The compositing switch, with what it means in its title. */
export function mixSwitch(show) {
  const on = mixed(show);
  const busy = !!show.switching;
  const title = busy ? "Switching: the outputs are moving over. This takes up to half a minute."
    : on ? "Mixed: scenes, transitions and a programme encode. Click to send the input straight to the outputs." : "Direct: the input goes straight to the outputs. Click to mix it with scenes and transitions.";
  return el("button.wl-switch", {
    type: "button", role: "switch", "aria-checked": String(on), "aria-busy": String(busy), "data-act": "mix", "data-id": show.id, tabindex: "-1", title,
  }, [el("span.wl-knob"), el("span.wl-swword", { text: busy ? "Switching" : on ? "Mixed" : "Direct" })]);
}

/** The input's numbers: transport and shape on one line, health on the next. */
export function inputCell(show, st) {
  const i = (st && st.input) || {};
  const t = transport(show.input && show.input.uri) || (mixed(show) ? "Scenes" : "");
  const line = [t, format(i)].filter(Boolean).join(" · ");
  const audio = i.audio_codec ? `${codec(i.audio_codec)}${i.audio_channels ? ` ${i.audio_channels}ch` : ""}` : "";
  return el("span.wl-in", { title: show.input ? show.input.uri : "" }, [el("span.wl-line", { text: line || "Waiting for the input" }), el("span.wl-sub", { text: audio || (show.input ? show.input.uri : "") })]);
}

export function rateCell(st, history) {
  const i = (st && st.input) || {};
  const errs = [];
  if (i.packets_lost) errs.push(el("span.wl-bad", { text: `${i.packets_lost} lost` }));
  if (i.cc_errors) errs.push(el("span.wl-bad", { text: `${i.cc_errors} CC` }));
  return el("span.wl-rate", {}, [
    el("span.wl-rateline", {}, [sparkline(history), el("span.wl-num", { text: st ? kbps(i.kbps || 0) : "" })]),
    el("span.wl-sub", {}, errs.length ? errs : [el("span", { text: st ? "no loss" : "" })]),
  ]);
}

const LOAD_TITLES = {
  mix: "This show mixes: what its process, compositing and encoding, costs this machine",
  transcode: "What this show's encodes cost on this machine",
  copy: "This show copies its input to its outputs and encodes nothing",
};

/** What the show does: the core's `work`, or worked out for a core from before it. */
export function workOf(show, st) {
  if (st && st.work) return st.work;
  if (mixed(show)) return "mix";
  return ((st && st.outputs) || []).some((o) => o.encoder) ? "transcode" : "copy";
}

export function loadText(show, st) {
  if (!st) return "";
  const work = workOf(show, st);
  if (work === "copy") return "copy only";
  const m = load(st);
  if (!m) return work === "mix" ? "mixing" : "encoding";
  return m < 1000 ? `${Math.max(1, Math.round(m / 10))}% core` : `${(m / 1000).toFixed(1)} cores`;
}

export function loadCell(show, st) {
  return el("span.wl-num.wl-load", { text: loadText(show, st), title: st ? LOAD_TITLES[workOf(show, st)] || "" : "" });
}
