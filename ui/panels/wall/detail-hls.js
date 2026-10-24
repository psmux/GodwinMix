// The HLS settings of a direct show's hls:// output, in its detail: low
// latency as one click, and the rest (segments, who may play it, segment
// length, the window) under Advanced. Used to add one and to change one;
// `show.output.set` replaces every param, so the whole set is always sent.

import { el } from "../../shell/dom.js";
import { copy } from "../channels/keyed.js";

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

/**
 * The playlists a player opens, CMAF and MPEG-TS, as whole addresses with
 * the viewer key, from the output's own `playback`; null until it has made
 * one. A tester found no link at all for a direct show's HLS output. Each
 * says whether the output makes it: TS only with segments ts or both.
 * The show is the output's own, in the path it was given, never the page's.
 */
export function playlists(o, base = location.origin) {
  const path = o && o.playback && o.playback.master_url_path;
  if (!path) return null;
  const seg = o.playback.segments || (o.params && o.params.segments) || "cmaf";
  return [
    { kind: "CMAF", url: new URL(path, base).href, made: seg !== "ts" },
    { kind: "MPEG-TS", url: new URL(path.replace("/master.m3u8", "/master-ts.m3u8"), base).href, made: seg !== "cmaf" },
  ];
}

/** The playlists as rows with a Copy each, for an output's row. */
export function playlistLinks(o, base) {
  const all = playlists(o, base);
  if (!all) return el("div.wl-dlinks", {}, [el("small.wl-sub", { text: "The playlist links appear once the output is on and its show is running." })]);
  return el("div.wl-dlinks", {}, all.map((p) => p.made
    ? el("div.wl-dlink", {}, [el("span.wl-sub", { text: p.kind }), el("code", { text: p.url, title: p.url }), el("button.btn", { type: "button", text: "Copy", "aria-label": `Copy the ${p.kind} playlist link`, onclick: (e) => copy(e.currentTarget, p.url) })])
    : el("div.wl-dlink", {}, [el("span.wl-sub", { text: p.kind }), el("small.wl-sub", { text: p.kind === "CMAF" ? "Not made: this output makes MPEG-TS segments only." : "Not made: set Segments to MPEG-TS, or both, under HLS settings." })])));
}
