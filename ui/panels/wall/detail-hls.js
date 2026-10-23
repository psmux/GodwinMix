// The HLS settings of a direct show's hls:// output, in its detail: low
// latency as one click, and the rest (segments, who may play it, segment
// length, the window) under Advanced. Used to add one and to change one;
// `show.output.set` replaces every param, so the whole set is always sent.

import { el } from "../../shell/dom.js";

/** True for an output the station serves as HLS. */
export const isHls = (o) => !!o && (o.platform === "hls" || /^hls:\/\//i.test(o.uri || o.uri_host || ""));

/**
 * @param {object|null} params what the output has now (`params` in show.list),
 *   or null for a new one or one that is off
 * @returns {Promise<{node: HTMLElement, read: () => object}>}
 */
export async function hlsBlock(params) {
  const { hlsAdvanced } = await import("../renditions/hls-settings.js");
  const adv = await hlsAdvanced({ direct: true, params });
  const low = el("input", { type: "checkbox", checked: !!(params && params.part_ms > 0) });
  const node = el("div.wl-dhls", {}, [
    el("label.wl-dcheck", {}, [low, " Low latency, about two seconds behind, for players that support it"]),
    adv.el,
  ]);
  return { node, read: () => adv.read(low.checked) };
}

/** What an HLS output says in its row: the segments and who may play it. */
export function hlsWords(o) {
  const p = (o && o.params) || {};
  const seg = { ts: "MPEG-TS", both: "CMAF and TS" }[p.segments] || "";
  const who = p.access === "open" ? (p.open_public ? "open to all" : "open on the network") : "";
  return [seg, who, p.segment_ms && p.segment_ms !== 2000 ? `${(p.segment_ms / 1000).toFixed(1)} s segments` : ""].filter(Boolean).join(", ");
}
