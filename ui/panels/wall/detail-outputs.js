// A direct show's outputs in its detail: each with its state, what it is
// sent as and its bitrate; change its format with the same Format step the
// Add destination form uses, turn it off, remove it, or add another.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { outFormat, outName } from "./cells.js";
import { kbps } from "./model.js";

/** The input as a StreamInfo, so the format step can say when copy is enough. */
export function inputShape(st) {
  const v = st && st.input;
  if (!v || !v.width) return null;
  return { encoded: true, video: { codec: v.video_codec, width: v.width, height: v.height, fps: { num: Math.round((v.fps || 25) * 1000), den: 1000 }, bitrate_kbps: v.kbps || 0 } };
}

/**
 * @param {object} client
 * @param {() => object} show the show as it is now
 * @param {() => object|null} stats its last show.stats, or null
 * @param {() => Promise<void>} reread
 */
export function outputsSection(client, show, stats, reread) {
  const list = el("div.wl-douts");
  const slot = el("div.wl-dform");
  const call = async (method, params, what) => {
    try {
      await client.call(method, { show: show().id, ...params });
      await reread();
    } catch (e) {
      errorToast(e, what);
    }
  };
  const formatFor = async (current, title, editing) => {
    const { formatStep } = await import("../renditions/format-step.js");
    const step = formatStep(client, { current, shape: inputShape(stats()), platformTitle: title });
    const save = el("button.btn.primary", { type: "button", text: editing ? "Save format" : "Add output" });
    const cancel = el("button.btn", { type: "button", text: "Cancel", onclick: () => slot.replaceChildren() });
    return { step, save, cancel };
  };

  async function change(o) {
    const f = await formatFor(o.rendition || undefined, outName(o), true);
    f.save.onclick = () => {
      const v = f.step.value();
      slot.replaceChildren();
      if (v !== undefined || o.rendition) call("show.output.set", { id: o.id, rendition: v === undefined ? o.rendition : v }, `${outName(o)} keeps its format`);
    };
    slot.replaceChildren(el("div.wl-dbox", {}, [f.step.node, el("div.wl-dbtns", {}, [f.cancel, f.save])]));
  }

  async function add() {
    const uri = el("input", { type: "text", placeholder: "udp://10.0.0.50:6000, srt://host:port, rtmp://host/app/key", "aria-label": "Where to send it", spellcheck: "false" });
    const f = await formatFor(undefined, "", false);
    f.save.onclick = () => {
      if (!uri.value.trim()) return uri.focus();
      const v = f.step.value();
      slot.replaceChildren();
      call("show.output.add", { uri: uri.value.trim(), rendition: v === undefined ? null : v }, "The output was not added");
    };
    slot.replaceChildren(el("div.wl-dbox", {}, [el("label.wl-dfield", {}, [el("span", { text: "Send to" }), uri]), f.step.node, el("div.wl-dbtns", {}, [f.cancel, f.save])]));
    uri.focus();
  }

  function draw() {
    const s = show();
    const by = new Map(((stats() && stats().outputs) || []).map((o) => [o.id, o]));
    const outs = s.outputs || [];
    list.replaceChildren(...outs.map((o) => {
      const st = by.get(o.id);
      const state = (st && st.state) || o.state || "idle";
      return el(`div.wl-dout.${state}`, {}, [
        el("span.wl-dot"),
        el("span.wl-doutname", {}, [el("strong", { text: outName(o) }), el("span.wl-sub", { text: o.uri || "platform" })]),
        el("span.wl-doutfmt", { text: `${outFormat(o, st)}${st && st.kbps ? ` · ${kbps(st.kbps)}` : ""} · ${o.enabled === false ? "off" : state}` }),
        el("span.wl-doutbtns", {}, [
          el("button.btn", { type: "button", text: "Format", onclick: () => change(o) }),
          el("button.btn", { type: "button", text: o.enabled === false ? "Turn on" : "Turn off", onclick: () => call("show.output.set", { id: o.id, enabled: o.enabled === false }, `${outName(o)} did not change`) }),
          el("button.btn.icon", { type: "button", text: "×", title: `Remove ${outName(o)}`, "aria-label": `Remove ${outName(o)}`, onclick: () => call("show.output.remove", { id: o.id }, `${outName(o)} was not removed`) }),
        ]),
      ]);
    }));
    if (!outs.length) list.append(el("p.wl-calm", { text: "No outputs yet. The input is read but sent nowhere." }));
  }

  draw();
  const node = el("section.wl-dsec", {}, [
    el("div.wl-dhead", {}, [el("h3", { text: "Outputs" }), el("button.btn", { type: "button", text: "Add output", onclick: add })]),
    list, slot,
  ]);
  return { node, draw };
}
