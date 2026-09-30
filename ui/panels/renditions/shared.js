// What every rendition view reads from the core, and its stylesheet.
//
// Everything under panels/renditions is fetched the first time a format is
// chosen, a plan line is wanted or Resources is opened. A core that does not
// have the rendition methods yet answers "no such method", and every caller
// then carries on exactly as the page did before renditions existed.

import { el } from "../../shell/dom.js";

const CSS_ID = "gmx-renditions-css";
const NO_METHOD = -32601;

export function stylesheet() {
  if (document.getElementById(CSS_ID)) return;
  document.head.appendChild(el("link#" + CSS_ID, { rel: "stylesheet", href: new URL("./renditions.css", import.meta.url).href }));
}

/** True for the answer of a core that has no such method. */
export const missing = (e) => !!e && e.code === NO_METHOD;

/**
 * `rendition.presets`, or null on a core without it. Asked each time a form
 * opens: it is one small answer, and a GPU that went away since the last one
 * must not still be offered.
 */
export async function loadPresets(client) {
  try {
    const got = await client.call("rendition.presets", {});
    return (got && got.presets) || [];
  } catch (e) {
    if (!missing(e)) console.debug("rendition.presets", e);
    return null;
  }
}

/** How much CPU the governor would still admit, for the cost badges. */
export async function roomNow(client) {
  try {
    const s = await client.call("governor.status", {});
    return (s && s.cpu && s.cpu.room_millicores) || 0;
  } catch {
    return 0;
  }
}

/**
 * The programme's own encode as a StreamInfo, from the mixer's settings, so
 * the format step can tell when the programme already is what a platform
 * wants. Null when this token may not read the settings.
 */
export async function programmeShape(client) {
  const keys = ["canvas.width", "canvas.height", "canvas.fps", "program.video_bitrate_kbps"];
  try {
    const got = await client.call("config.get", { keys });
    const v = Object.fromEntries(((got && got.keys) || []).map((k) => [k.key, k.value]));
    if (!v["canvas.width"]) return null;
    return {
      encoded: false,
      video: {
        codec: "h264",
        width: v["canvas.width"],
        height: v["canvas.height"],
        fps: { num: v["canvas.fps"] || 30, den: 1 },
        bitrate_kbps: v["program.video_bitrate_kbps"] || 0,
      },
    };
  } catch {
    return null;
  }
}

/** A channel's first live stream as a StreamInfo, or null while nothing publishes. */
export function channelShape(channel) {
  const s = ((channel && channel.streams) || []).find((x) => x.video);
  if (!s) return null;
  const v = s.video;
  const fps = typeof v.fps === "number" ? { num: Math.round(v.fps * 1000), den: 1000 } : v.fps;
  return { encoded: true, video: { codec: v.codec, width: v.width, height: v.height, fps, bitrate_kbps: v.kbps || v.bitrate_kbps || 0 } };
}

/**
 * `rendition.plan` for one scope, kept current while `onPlan` is wanted.
 * Asks for `rendition.*` events only until the returned function is called,
 * which the caller does the moment its view leaves the screen.
 */
export function followPlan(client, scope, onPlan) {
  let live = true;
  const params = scope && scope !== "programme" ? { scope } : {};
  const read = async () => {
    try {
      const plan = await client.call("rendition.plan", params);
      if (live) onPlan(plan);
    } catch (e) {
      // A core without the planner: stop asking for its events at once.
      if (missing(e)) release();
      else if (live) console.debug("rendition.plan", e);
    }
  };
  let held = client.listen ? client.listen("rendition.*") : () => {};
  const release = () => {
    held();
    held = () => {};
  };
  const offs = [
    client.on("event", ({ name, params: p }) => {
      if (name !== "rendition.plan" || !live) return;
      const same = (p && (p.scope || "programme")) === (scope || "programme");
      if (same) onPlan(p.plan);
    }),
    client.on("open", read),
  ];
  read();
  return () => {
    live = false;
    release();
    for (const off of offs) off();
  };
}
