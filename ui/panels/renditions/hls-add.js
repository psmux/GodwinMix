// "HLS for viewers": the programme as a link a phone or a smart TV can play,
// in several sizes so a weak connection still gets a picture. Served from
// the mixer's own port, so there is nothing to open on the router.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { ladderPicker } from "./ladder.js";
import { hlsAdvanced, latencyDefaults, paramsFrom } from "./hls-settings.js";
import { loadPresets, stylesheet } from "./shared.js";

/** True when this core can make an HLS output. */
export async function canServeHls(client) {
  try {
    const api = await client.call("core.api", {});
    return ((api.kinds && api.kinds.output) || []).some((k) => k.id === "hls/output");
  } catch {
    return false;
  }
}

/** The tile for Add destination, or null on a core without HLS. */
export async function hlsTile(client, closePicker, onDone) {
  if (!(await canServeHls(client))) return null;
  return el("button.kindtile.rnd-hlstile", {
    onclick: () => {
      closePicker();
      addHls(client, onDone);
    },
  }, [el("span.grow", {}, [el("div", { text: "HLS for viewers" }), el("div.dim.sm", { text: "A link people open on a phone, a laptop or a TV, in several sizes. No platform, no key." })])]);
}

/** The params of the add, or `{error}`. `params` are the Advanced answers'. */
export function hlsParams(id, ladder, lowLatency, params) {
  const name = String(id || "").trim();
  if (!/^[a-z0-9][a-z0-9-]*$/.test(name)) return { error: "Give it a short name of small letters, numbers and dashes." };
  if (ladder.error) return { error: ladder.error };
  return { params: { id: name, type: "hls/output", rendition: ladder, params: params || paramsFrom({}, lowLatency) } };
}

/**
 * The params an edit sends to `output.set`: the ladder and every HLS setting,
 * spelled out. `output.set` lays what it is given over what the output has,
 * so a setting turned back to its default has to be said, or the old one
 * stays: no low latency is a part of 0, and keyed access is "keyed".
 */
export function hlsEditParams(id, ladder, params) {
  if (ladder.error) return { error: ladder.error };
  const p = { ...params };
  if (!p.part_ms) p.part_ms = 0;
  if (p.access !== "open") Object.assign(p, { access: "keyed", open_public: false });
  else if (!p.open_public) p.open_public = false;
  return { params: { id, rendition: ladder, ...p } };
}

/**
 * What an HLS output's status says of its settings, as the Advanced form
 * takes them. A segment and part the low latency click already gives are
 * left to the click, so unticking it is enough to turn low latency off.
 */
export function savedParams(output) {
  const o = output || {};
  const out = {};
  for (const k of ["segment_ms", "part_ms", "window", "access", "open_public"]) if (o[k] !== undefined && o[k] !== null) out[k] = o[k];
  const base = latencyDefaults(Number(out.part_ms) > 0);
  for (const k of ["segment_ms", "part_ms"]) if (out[k] === base[k]) delete out[k];
  return out;
}

export function addHls(client, onDone) {
  return hlsForm(client, null, onDone);
}

/** Change an HLS output that is already serving: the same form, opened on what it has. */
export function editHls(client, output, onDone) {
  return hlsForm(client, output, onDone);
}

async function hlsForm(client, output, onDone) {
  stylesheet();
  const editing = !!output;
  const presets = (await loadPresets(client)) || [];
  const picker = ladderPicker(presets, editing ? output.rendition : undefined);
  const saved = editing ? savedParams(output) : {};
  const name = el("input", { type: "text", value: editing ? output.id : "viewers", autocomplete: "off", spellcheck: false, disabled: editing });
  const low = el("input", { type: "checkbox", checked: editing && Number(output.part_ms) > 0 });
  const refused = el("div", { hidden: true });
  const start = el("button.btn.primary", { text: editing ? "Save" : "Start serving" });
  // MPEG-TS is made only by a show without compositing, so it is not offered here.
  const advanced = await hlsAdvanced({ direct: false, params: editing ? saved : undefined });
  const body = el("div.form.rnd-hlsform", {}, [
    el("p.dim.sm", { style: { marginTop: "0" }, text: editing ? "Saving rebuilds the output, so players load the link again after a few seconds." : "Players pick the size that suits their connection, and switch as it changes." }),
    el("label.field", {}, [el("span.lbl", { text: "Name" }), name, el("span.hint", { text: editing ? "Part of the link, so it stays as it is." : "It becomes part of the link." })]),
    el("section.rnd-step", {}, [el("div.rnd-steph", {}, [el("span.rnd-kicker", { text: "Sizes" }), el("span.rnd-dim", { text: "Each one is a separate encode" })]), picker.node]),
    el("label.rnd-check", {}, [low, el("span", { text: "Low latency (about two seconds behind, for players that support it)" })]),
    advanced.el,
    refused,
  ]);
  const m = modal({ title: editing ? `Edit ${output.id}` : "HLS for viewers", body, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), start] });

  async function send(params) {
    try {
      await client.call(editing ? "output.set" : "output.add", params);
    } catch (e) {
      const { isRefusal, showRefusal } = await import("./refusal.js");
      if (!isRefusal(e)) {
        errorToast(e, editing ? `Edit ${output.id}` : "HLS for viewers");
        return false;
      }
      // Advice for a ladder is one smaller rendition; it replaces the ladder.
      showRefusal(refused, e, (request) => send({ ...params, rendition: request }));
      return false;
    }
    m.close();
    toast({ text: editing ? `${params.id} saved. Players reload the link in a few seconds.` : `Serving ${params.id}. The link is on its row under Outputs.` });
    if (onDone) await onDone();
    return true;
  }

  start.onclick = async () => {
    const asked = editing
      ? hlsEditParams(output.id, picker.value(), advanced.read(low.checked))
      : hlsParams(name.value, picker.value(), low.checked, advanced.read(low.checked));
    if (asked.error) return toast({ kind: "warning", text: asked.error });
    start.disabled = true;
    if (!(await send(asked.params))) start.disabled = false;
  };
  return m;
}
