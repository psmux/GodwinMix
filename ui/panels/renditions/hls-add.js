// "HLS for viewers": the programme as a link a phone or a smart TV can play,
// in several sizes so a weak connection still gets a picture. Served from
// the mixer's own port, so there is nothing to open on the router.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { ladderPicker } from "./ladder.js";
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

/** The params of the add, or `{error}`. */
export function hlsParams(id, ladder, lowLatency) {
  const name = String(id || "").trim();
  if (!/^[a-z0-9][a-z0-9-]*$/.test(name)) return { error: "Give it a short name of small letters, numbers and dashes." };
  if (ladder.error) return { error: ladder.error };
  const params = { segment_ms: lowLatency ? 1000 : 2000, window: 30 };
  if (lowLatency) params.part_ms = 200;
  return { params: { id: name, type: "hls/output", rendition: ladder, params } };
}

export async function addHls(client, onDone) {
  stylesheet();
  const presets = (await loadPresets(client)) || [];
  const picker = ladderPicker(presets);
  const name = el("input", { type: "text", value: "viewers", autocomplete: "off", spellcheck: false });
  const low = el("input", { type: "checkbox" });
  const refused = el("div", { hidden: true });
  const start = el("button.btn.primary", { text: "Start serving" });
  const body = el("div.rnd-hlsform", {}, [
    el("p.dim.sm", { style: { marginTop: "0" }, text: "Players pick the size that suits their connection, and switch as it changes." }),
    el("label.field", {}, [el("span.lbl", { text: "Name" }), name, el("span.hint", { text: "It becomes part of the link." })]),
    el("section.rnd-step", {}, [el("div.rnd-steph", {}, [el("span.rnd-kicker", { text: "Sizes" }), el("span.rnd-dim", { text: "Each one is a separate encode" })]), picker.node]),
    el("label.rnd-check", {}, [low, el("span", { text: "Low latency (about two seconds behind, for players that support it)" })]),
    refused,
  ]);
  const m = modal({ title: "HLS for viewers", body, wide: true, footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), start] });

  async function send(params) {
    try {
      await client.call("output.add", params);
    } catch (e) {
      const { isRefusal, showRefusal } = await import("./refusal.js");
      if (!isRefusal(e)) {
        errorToast(e, "HLS for viewers");
        return false;
      }
      // Advice for a ladder is one smaller rendition; it replaces the ladder.
      showRefusal(refused, e, (request) => send({ ...params, rendition: request }));
      return false;
    }
    m.close();
    toast({ text: `Serving ${params.id}. The link is on its row under Outputs.` });
    if (onDone) await onDone();
    return true;
  }

  start.onclick = async () => {
    const asked = hlsParams(name.value, picker.value(), low.checked);
    if (asked.error) return toast({ kind: "warning", text: asked.error });
    start.disabled = true;
    if (!(await send(asked.params))) start.disabled = false;
  };
  return m;
}
