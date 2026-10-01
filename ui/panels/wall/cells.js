// The pieces a row and a tile are made of: a sparkline, an output chip, an
// alarm with its age, the compositing switch. Plain nodes, no state.

import { el } from "../../shell/dom.js";
import { ALARMS, age, codec, format, healthOf, kbps, load, transport } from "./model.js";

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

export function outName(o) {
  if (o.platform) return o.platform;
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
  const on = !!show.compositing;
  return el("button.wl-switch", {
    type: "button", role: "switch", "aria-checked": String(on), "data-act": "mix", "data-id": show.id, tabindex: "-1",
    title: on ? "Mixed: scenes, transitions and a programme encode. Click to send the input straight to the outputs." : "Direct: the input goes straight to the outputs. Click to mix it with scenes and transitions.",
  }, [el("span.wl-knob"), el("span.wl-swword", { text: on ? "Mixed" : "Direct" })]);
}

/** The input's numbers: transport and shape on one line, health on the next. */
export function inputCell(show, st) {
  const i = (st && st.input) || {};
  const t = transport(show.input && show.input.uri) || (show.compositing ? "Scenes" : "");
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

export function loadCell(st) {
  const m = load(st);
  const encodes = ((st && st.outputs) || []).some((o) => o.encoder);
  const text = !st ? "" : !encodes ? "copy only" : m < 1000 ? `${Math.max(1, Math.round(m / 10))}% core` : `${(m / 1000).toFixed(1)} cores`;
  return el("span.wl-num.wl-load", { text, title: "What this show's encodes cost on this machine" });
}
