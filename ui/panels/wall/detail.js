// One direct show, opened from its row on the wall: its input and a backup
// for it, its outputs (detail-outputs.js), its alarm thresholds, and the
// compositing switch with what it does. Every change is show.set or
// show.output.*, sent when its Save is pressed.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { outputsSection } from "./detail-outputs.js";
import { ALARMS, age, format, healthOf, kbps, transport } from "./model.js";

const MIX_ON = "Mixed: this show runs as a mixer of its own, with scenes, transitions and one programme encode that every output takes.";
const MIX_OFF = "Direct: the input goes straight to the outputs with no mixer and no decode, unless an output asks for a new format.";

function sheet() {
  if (document.getElementById("gmx-wall-css")) return;
  document.head.append(el("link#gmx-wall-css", { rel: "stylesheet", href: new URL("./wall.css", import.meta.url).href }));
}

const field = (label, input, hint) => el("label.wl-dfield", {}, [el("span", { text: label }), input, hint ? el("small.wl-sub", { text: hint }) : null]);
const text = (value, placeholder, label) => el("input", { type: "text", value: value || "", placeholder, "aria-label": label, spellcheck: "false" });
const num = (value, label, step = 1) => el("input.wl-dnum", { type: "number", value: String(value ?? ""), step: String(step), "aria-label": label });

export function showDetail(client, show, opts = {}) {
  sheet();
  let current = show;
  let stats = (opts.data && opts.data.stats.get(show.id)) || null;
  const send = async (patch, what, said) => {
    try {
      const got = opts.data ? await opts.data.set(current.id, patch) : await client.call("show.set", { id: current.id, ...patch });
      if (got && got.id) current = { ...current, ...got };
      if (said) toast({ text: said });
      return true;
    } catch (e) {
      errorToast(e, what);
      return false;
    }
  };
  const reread = async () => {
    const got = await client.call("show.list", {}).catch(() => null);
    const fresh = got && (got.shows || []).find((s) => s.id === current.id);
    if (fresh) current = fresh;
    outs.draw();
  };
  const outs = outputsSection(client, () => current, () => stats, reread);
  const live = el("p.wl-dlive");
  const body = el("div.wl-detail", {}, [live, mixing(current, send), input(current, send), outs.node, alarms(current, send)]);
  const dlg = modal({ title: show.name, body, wide: true });
  const drawLive = () => live.replaceChildren(...liveWords(current, stats));
  const read = async () => {
    if (!dlg.el.isConnected) return clearInterval(timer);
    const got = await client.call("show.stats", { ids: [current.id] }).catch(() => null);
    stats = (got && got.shows && got.shows[0]) || stats;
    drawLive();
    outs.draw();
  };
  const timer = setInterval(read, 2000);
  read();
  return { dialog: dlg, close: () => { clearInterval(timer); dlg.close(); } };
}

function liveWords(show, st) {
  const h = healthOf(show);
  const i = (st && st.input) || {};
  const bits = [el(`span.wl-health.${h.state}`), el("strong", { text: { alarm: "In alarm", warning: "Warning", ok: "Running", off: "Off" }[h.state] || h.state })];
  const nums = [transport(show.input && show.input.uri), format(i), i.kbps ? kbps(i.kbps) : "", i.cc_errors ? `${i.cc_errors} CC errors` : "", i.packets_lost ? `${i.packets_lost} packets lost` : ""].filter(Boolean);
  if (nums.length) bits.push(el("span", { text: nums.join(" · ") }));
  for (const a of h.alarms) bits.push(el("span.wl-alarm", {}, [el("span", { text: ALARMS[a.kind] || a.kind }), el("span.wl-age", { text: age(a.since_ms) })]));
  return bits;
}

function mixing(show, send) {
  const sw = el("button.wl-switch.big", { type: "button", role: "switch", "aria-checked": String(!!show.compositing), "aria-label": "Mixing" }, [el("span.wl-knob"), el("span.wl-swword", { text: show.compositing ? "Mixed" : "Direct" })]);
  const said = el("p.wl-sub.wl-wrap", { text: show.compositing ? MIX_ON : MIX_OFF });
  sw.onclick = async () => {
    const on = sw.getAttribute("aria-checked") !== "true";
    const ok = await send({ compositing: on }, `${show.name} stays ${on ? "direct" : "mixed"}`, on ? `${show.name} is mixed now.` : `${show.name} is direct now.`);
    const now = ok ? on : !on;
    sw.setAttribute("aria-checked", String(now));
    sw.lastChild.textContent = now ? "Mixed" : "Direct";
    said.textContent = now ? MIX_ON : MIX_OFF;
  };
  return el("section.wl-dsec.wl-dmix", {}, [el("div.wl-dhead", {}, [el("h3", { text: "Mixing" }), sw]), said]);
}

function input(show, send) {
  const i = show.input || {};
  const uri = text(i.uri, "udp://@239.1.1.1:5000", "Input address");
  const program = num(i.program, "Program number");
  program.placeholder = "first";
  const backup = text(i.backup && i.backup.uri, "None. srt://backup-host:9000, or another multicast group", "Backup input address");
  const save = el("button.btn.primary", { type: "button", text: "Save input", onclick: () => {
    const next = { uri: uri.value.trim() };
    if (Number(program.value) > 0) next.program = Number(program.value);
    if (backup.value.trim()) next.backup = { uri: backup.value.trim() };
    send({ input: next }, "The input did not change", "Saved. The input is read again from the new address.");
  } });
  return el("section.wl-dsec", {}, [
    el("div.wl-dhead", {}, [el("h3", { text: "Input" })]),
    el("div.wl-dgrid", {}, [field("Address", uri), field("Program", program, "When a feed carries several")]),
    field("Backup input", backup, "Taken when the main input stalls, and let go when it comes back."),
    el("div.wl-dbtns", {}, [save]),
  ]);
}

function alarms(show, send) {
  const a = { enabled: true, black_ms: 5000, freeze_ms: 10000, silence_ms: 10000, silence_dbfs: -60, ...(show.alarms || {}) };
  const on = el("input", { type: "checkbox", checked: a.enabled !== false });
  const black = num(a.black_ms / 1000, "Black after, in seconds");
  const freeze = num(a.freeze_ms / 1000, "Frozen after, in seconds");
  const silence = num(a.silence_ms / 1000, "Silent after, in seconds");
  const dbfs = num(a.silence_dbfs, "Silent below, in dBFS");
  const save = el("button.btn", { type: "button", text: "Save alarms", onclick: () => send({ alarms: {
    enabled: on.checked, black_ms: Number(black.value) * 1000, freeze_ms: Number(freeze.value) * 1000, silence_ms: Number(silence.value) * 1000, silence_dbfs: Number(dbfs.value),
  } }, "The alarms did not change", on.checked ? "Saved. The alarms use the new times." : "Saved. This show raises no picture or sound alarms now.") });
  return el("section.wl-dsec", {}, [
    el("div.wl-dhead", {}, [el("h3", { text: "Alarms" }), el("label.wl-dcheck", {}, [on, " Watch the picture and sound"])]),
    el("p.wl-sub.wl-wrap", { text: "Watching costs a keyframe decode about once a second and a little audio, even with the wall closed. Without it, a show still raises no input, stall, loss and output alarms." }),
    el("div.wl-dgrid.four", {}, [field("Black after (s)", black), field("Frozen after (s)", freeze), field("Silent after (s)", silence), field("Silent below (dBFS)", dbfs)]),
    el("div.wl-dbtns", {}, [save]),
  ]);
}
