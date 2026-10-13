// What one destination sends, as against what comes into its channel. A
// tester set a Custom destination to 720p at 3000 kb/s and read the channel's
// own 1920x1080 6.3 Mb/s as what the platform got; the channel line is what
// arrives, and these words are what leaves for one destination.
//
// The size, rate and codecs come from the plan the core made for the
// destination (`d.plan`), which is what its encoder was built to: the scale
// node holds the picture to that size and rate, so the plan is what goes out.
// The bitrate is the one measured on the wire (`d.kbps`). Pure, no DOM.

import { codecName, fmtKbps } from "./model.js";

/** 30 fps, 29.97 fps, from the wire's fraction. */
export function planFps(fps) {
  if (!fps || !fps.num) return "";
  const f = fps.num / (fps.den || 1);
  return `${Math.round(f * 100) / 100} fps`;
}

/** The picture as it goes out: "1280×720 30 fps H.264", or "". */
export function sendsVideo(v) {
  if (!v) return "";
  const size = v.width ? `${v.width}×${v.height}` : "";
  return [size, planFps(v.fps), codecName(v.codec)].filter(Boolean).join(" ");
}

/** The sound as it goes out: "AAC 128 kb/s", or "". */
export function sendsAudio(a) {
  if (!a) return "";
  return [codecName(a.codec), a.bitrate_kbps ? fmtKbps(a.bitrate_kbps) : ""].filter(Boolean).join(" ");
}

/**
 * The words for one destination: `short` for its tile, `line` for its
 * dialog, both empty while the core has no plan for it (switched off,
 * waiting for its stream, or sent as it arrives with nothing asked).
 */
export function sends(d) {
  const p = d && d.plan;
  if (!p || (!p.video && !p.audio)) return { short: "", line: "" };
  const video = sendsVideo(p.video);
  const audio = sendsAudio(p.audio);
  const short = video || audio;
  const asked = p.video && p.video.bitrate_kbps ? `video at ${fmtKbps(p.video.bitrate_kbps)}` : "";
  const what = [video && [video, asked].filter(Boolean).join(", "), audio].filter(Boolean).join("; ");
  const how = p.mode === "copy" ? "Sends what comes in, copied" : "Sends";
  const measured = d.state === "live" && d.kbps ? `. Measured going out: ${fmtKbps(d.kbps)}` : "";
  return { short, line: `${how}: ${what}${measured}` };
}
