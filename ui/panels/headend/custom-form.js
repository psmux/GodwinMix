// The custom settings of a conversion, as a form: the channel editor's
// Custom settings and the profile editor's (profiles.js). The encoder
// choice comes from the machine's encoders, so GPU is offered only where
// there is one. Rare things are under Advanced, which opens by itself
// when the settings use one of them.

import { el } from "../../shell/dom.js";
import { SIZES, convFields, wantFrom } from "./conv-fields.js";
import { ASPECT, FPS, MATCH, METHOD, withValue } from "./picture.js";
import { ladderBox } from "./ladder-box.js";
import { choice, field, num, text } from "./form.js";

const FIT = [["pad", "Pad with bars"], ["crop", "Crop to fill"], ["stretch", "Stretch to fill"]];
const DEINT = [["auto", "Only pictures marked interlaced"], ["on", "Every picture"], ["off", "Off"]];
const TRACKS = [["first", "The first track only"], ["all", "Every track, each on its own"]];
const AUDIO = [["copy", "As it comes"], ["aac", "AAC"], ["opus", "Opus"], ["mp3", "MP3"], ["ac3", "AC-3"]];
const CPU_ONLY = "Runs on the CPU whatever the method: GStreamer has no GPU deinterlace. The scale and the encode stay on the GPU.";

/** The encoder choices this machine has: Auto always, GPU when there is one. */
export function encoderChoices(encoders, current) {
  const has = (on) => (encoders || []).some((e) => e.on === on);
  const rows = [["auto", "Auto, the GPU first"]];
  if (has("gpu") || current === "gpu") rows.push(["gpu", has("gpu") ? "GPU only" : "GPU only (this machine has none)"]);
  rows.push(["cpu", "CPU only"]);
  return rows;
}

/** The custom settings, as a form. `p` is a profile or {video, audio, ladder}. */
export function customForm(p, encoders) {
  const f = convFields(p);
  const codecs = [["copy", "Picture as it comes"], ["h264", "H.264"]];
  if ((encoders || []).some((e) => e.codec === "h265") || f.codec === "h265") codecs.push(["h265", "HEVC (H.265)"]);
  const c = {
    codec: choice(codecs, f.codec, "Video codec"),
    size: choice(SIZES, f.size, "Picture size"),
    fit: choice(FIT, f.fit, "Another shape"),
    pad_color: el("input", { type: "color", value: f.pad_color, "aria-label": "Bar colour" }),
    bitrate_kbps: num(f.bitrate_kbps, "1500", "Video bitrate", { min: "100", step: "100" }),
    fps: choice(withValue(FPS, f.fps), f.fps, "Frame rate"),
    keyframe_s: num(f.keyframe_s, "2", "Keyframe every, in seconds", { min: "0.5", step: "0.5" }),
    keyframe_frames: num(f.keyframe_frames, "", "Keyframe every, in frames", { min: "1" }),
    deinterlace: choice(DEINT, f.deinterlace, "Deinterlace"),
    deinterlace_method: choice(METHOD, f.deinterlace_method, "Deinterlace method"),
    match_height: choice(MATCH, f.match_height, "HD sources at their own size"),
    match_kbps: num(f.match_kbps, "4500", "Bitrate at the larger size", { min: "100", step: "100" }),
    aspect: choice(ASPECT, f.aspect, "Source shape"),
    encoder: choice(encoderChoices(encoders, f.encoder), f.encoder, "Encoder"),
    preset: text(f.preset, "p1 on NVENC, veryfast on x264", "Encoder preset"),
    bframes: num(f.bframes, "0", "B frames", { min: "0", max: "4" }),
    audio: choice(AUDIO, f.audio, "Sound codec"),
    audio_kbps: num(f.audio_kbps, "128", "Sound bitrate", { min: "32", step: "16" }),
    channels: num(f.channels, "as it comes", "Sound channels", { min: "1", max: "8" }),
    tracks: choice(TRACKS, f.tracks, "Sound tracks"),
  };
  const video = el("div.hs-grid", {}, [
    field("Video", c.codec),
    field("Size", c.size),
    field("Another shape", c.fit, "How a picture of another shape goes in."),
    field("Bar colour", c.pad_color),
    field("Bitrate, kb/s", c.bitrate_kbps),
    field("Frame rate", c.fps),
    field("Keyframe every, s", c.keyframe_s, "Two seconds suits HLS."),
    field("Deinterlace", c.deinterlace),
    field("Deinterlace method", c.deinterlace_method, CPU_ONLY),
    field("HD sources at their own size", c.match_height, "A source taller than the size above is converted at its own height, up to this, in its own shape. Smaller ones at the size above."),
    field("Bitrate at that height, kb/s", c.match_kbps, "Empty: the bitrate above grown by the picture's area."),
    field("Encoder", c.encoder),
  ]);
  const fine = el("details.hs-more", {}, [el("summary", { text: "Advanced" }), el("div.hs-grid", {}, [
    field("Source shape", c.aspect, "For a broadcaster that flags its picture wrong. Pad and crop fit by this shape."),
    field("Keyframe every, frames", c.keyframe_frames, "Taken over the seconds when set."),
    field("Preset", c.preset),
    field("B frames", c.bframes, "Left empty, none."),
  ])]);
  if (f.keyframe_frames || f.preset || f.bframes || f.aspect !== "auto") fine.open = true;
  const ladder = ladderBox(f.ladder, () => wantFrom({ ...values(), ladder: [] }).video || null);
  const ladderPart = el("div", {}, [el("h4.hs-sub", { text: "More sizes for HLS" }), ladder.node]);
  const sound = el("div.hs-grid", {}, [field("Sound", c.audio), field("Bitrate, kb/s", c.audio_kbps), field("Channels", c.channels), field("Tracks", c.tracks)]);
  const node = el("div.hs-custom", {}, [el("h4.hs-sub", { text: "Picture" }), video, fine, ladderPart, el("h4.hs-sub", { text: "Sound" }), sound]);
  const shown = (k, on) => { c[k].closest(".field").hidden = !on; };
  const sync = () => {
    const copy = c.codec.value === "copy";
    for (const k of ["size", "fit", "pad_color", "bitrate_kbps", "fps", "keyframe_s", "deinterlace", "encoder"]) shown(k, !copy);
    fine.hidden = ladderPart.hidden = copy;
    const sized = c.size.value !== "source";
    shown("fit", !copy && sized);
    shown("pad_color", !copy && sized && c.fit.value === "pad");
    shown("deinterlace_method", !copy && c.deinterlace.value !== "off");
    shown("match_height", !copy && sized);
    shown("match_kbps", !copy && sized && c.match_height.value !== "");
    const quiet = c.audio.value === "copy";
    for (const k of ["audio_kbps", "channels", "tracks"]) shown(k, !quiet);
  };
  function values() {
    const out = {};
    for (const [k, input] of Object.entries(c)) out[k] = input.value;
    return out;
  }
  node.addEventListener("change", sync);
  sync();
  return {
    node,
    controls: c,
    ladder,
    read: () => wantFrom({ ...values(), ladder: ladder.read() }),
  };
}
