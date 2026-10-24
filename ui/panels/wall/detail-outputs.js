// A direct show's outputs in its detail: each with its state, what it is
// sent as and its bitrate, and for HLS its CMAF and MPEG-TS playlists; Edit its address, key and format in the same
// form the Outputs panel uses (outputs/destination.js); change its format
// alone with the same Format step the
// Add destination form uses, an HLS output's settings (detail-hls.js), turn
// it off, remove it, or add another. A governor refusal stays in the form,
// with what would fit as buttons, rather than going by as a toast.

import { el } from "../../shell/dom.js";
import { errorToast } from "../../shell/toast.js";
import { outFormat, outName } from "./cells.js";
import { hlsBlock, hlsWords, isHls, playlistLinks } from "./detail-hls.js";
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
  const refusedIn = el("div", { hidden: true });
  // `form`: the call is a form's Save, which closes on success and keeps a
  // refusal inside it.
  const call = async (method, params, what, form) => {
    try {
      await client.call(method, { id: show().id, ...params });
      if (form) slot.replaceChildren();
      await reread();
      return true;
    } catch (e) {
      const { isRefusal, showRefusal } = await import("../renditions/refusal.js");
      if (!form || !isRefusal(e)) return errorToast(e, what), false;
      if (!refusedIn.isConnected) slot.prepend(refusedIn);
      showRefusal(refusedIn, e, (rendition) => call(method, { ...params, rendition }, what, form));
      return false;
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
      if (v === undefined && !o.rendition) return slot.replaceChildren();
      call("show.output.set", { output: o.id, rendition: v === undefined ? o.rendition : v }, `${outName(o)} keeps its format`, true);
    };
    slot.replaceChildren(el("div.wl-dbox", {}, [f.step.node, el("div.wl-dbtns", {}, [f.cancel, f.save])]));
  }

  async function edit(o) {
    const { editShowOutput } = await import("../outputs/destination.js");
    return editShowOutput(client, o, { show: show().id, shape: inputShape(stats()), onDone: reread });
  }

  async function settings(o) {
    // `params` has every setting; `playback` (older stations) the ones it serves.
    const p = o.params || (o.playback && o.playback.segments ? { segments: o.playback.segments, access: o.playback.access, segment_ms: o.playback.segment_ms } : null);
    const h = await hlsBlock(p);
    const off = p ? null : el("p.wl-sub.wl-wrap", { text: "This output is off, so what it was given is not read back. Saving gives it these settings." });
    const save = el("button.btn.primary", { type: "button", text: "Save settings", onclick: () => call("show.output.set", { output: o.id, params: h.read() }, `${outName(o)} keeps its settings`, true) });
    const cancel = el("button.btn", { type: "button", text: "Cancel", onclick: () => slot.replaceChildren() });
    slot.replaceChildren(el("div.wl-dbox", {}, [el("strong", { text: `HLS settings of ${o.uri_host || o.uri || o.id}` }), off, h.node, el("div.wl-dbtns", {}, [cancel, save])]));
  }

  async function add() {
    const uri = el("input", { type: "text", placeholder: "udp://10.0.0.50:6000, srt://host:port, rtmp://host/app/key", "aria-label": "Where to send it", spellcheck: "false" });
    const f = await formatFor(undefined, "", false);
    const h = await hlsBlock(null);
    const hls = () => /^hls:\/\//i.test(uri.value.trim());
    uri.addEventListener("input", () => { h.node.hidden = !hls(); });
    h.node.hidden = true;
    f.save.onclick = () => {
      if (!uri.value.trim()) return uri.focus();
      const v = f.step.value();
      const params = { uri: uri.value.trim(), rendition: v === undefined ? null : v };
      if (hls()) params.params = h.read();
      call("show.output.add", params, "The output was not added", true);
    };
    const hint = "udp://, srt:// and rtmp:// send it on; hls://name serves it as HLS from this station.";
    slot.replaceChildren(el("div.wl-dbox", {}, [el("label.wl-dfield", {}, [el("span", { text: "Send to" }), uri, el("small.wl-sub", { text: hint })]), h.node, f.step.node, el("div.wl-dbtns", {}, [f.cancel, f.save])]));
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
        el("span.wl-doutname", {}, [el("strong", { text: outName(o) }), el("span.wl-sub", { text: o.uri || o.uri_host || "platform" })]),
        el("span.wl-doutfmt", { text: [outFormat(o, st), isHls(o) ? hlsWords(o) : "", st && st.kbps ? kbps(st.kbps) : "", o.enabled === false ? "off" : state].filter(Boolean).join(" · ") }),
        el("span.wl-doutbtns", {}, [
          isHls(o) ? null : el("button.btn", { type: "button", text: "Edit", title: `Change where ${outName(o)} goes, its key or its format`, onclick: () => edit(o) }),
          el("button.btn", { type: "button", text: "Format", onclick: () => change(o) }),
          isHls(o) ? el("button.btn", { type: "button", text: "HLS settings", onclick: () => settings(o) }) : null,
          el("button.btn", { type: "button", text: o.enabled === false ? "Turn on" : "Turn off", onclick: () => call("show.output.set", { output: o.id, enabled: o.enabled === false }, `${outName(o)} did not change`) }),
          el("button.btn.icon", { type: "button", text: "×", title: `Remove ${outName(o)}`, "aria-label": `Remove ${outName(o)}`, onclick: () => call("show.output.remove", { output: o.id }, `${outName(o)} was not removed`) }),
        ]),
        isHls(o) ? playlistLinks(o, (client.transport && client.transport.base) || location.origin) : null,
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
