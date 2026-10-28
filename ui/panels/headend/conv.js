// A channel's conversion: a profile picked from the built in and saved
// ones, or custom settings of its own. The custom form is also the
// profile editor's (profiles.js). The encoder choice comes from the
// machine's encoders, so GPU is offered only where there is one.

import { el } from "../../shell/dom.js";
import { SIZES, convFields, profileWords, wantFrom } from "./setup.js";
import { choice, field, num, text } from "./form.js";

const FIT = [["pad", "Pad with bars"], ["crop", "Crop to fill"], ["stretch", "Stretch to fill"]];
const DEINT = [["auto", "Only pictures marked interlaced"], ["on", "Every picture"], ["off", "Off"]];
const TRACKS = [["first", "The first track only"], ["all", "Every track, each on its own"]];
export const STARTER = { video: { codec: "h264", width: 1280, height: 720, fit: "pad", bitrate_kbps: 1500, keyframe_ms: 2000, deinterlace: "auto" }, audio: { codec: "aac", bitrate_kbps: 128 } };
const AUDIO = [["copy", "As it comes"], ["aac", "AAC"], ["opus", "Opus"], ["mp3", "MP3"], ["ac3", "AC-3"]];

/** The encoder choices this machine has: Auto always, GPU when there is one. */
export function encoderChoices(encoders, current) {
  const has = (on) => (encoders || []).some((e) => e.on === on);
  const rows = [["auto", "Auto, the GPU first"]];
  if (has("gpu") || current === "gpu") rows.push(["gpu", has("gpu") ? "GPU only" : "GPU only (this machine has none)"]);
  rows.push(["cpu", "CPU only"]);
  return rows;
}

/** The custom settings, as a form. `p` is a profile or {video, audio}. */
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
    keyframe_s: num(f.keyframe_s, "2", "Keyframe every, in seconds", { min: "0.5", step: "0.5" }),
    keyframe_frames: num(f.keyframe_frames, "", "Keyframe every, in frames", { min: "1" }),
    deinterlace: choice(DEINT, f.deinterlace, "Deinterlace"),
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
    field("Keyframe every, s", c.keyframe_s, "Two seconds suits HLS."),
    field("Deinterlace", c.deinterlace),
    field("Encoder", c.encoder),
  ]);
  const fine = el("details.hs-more", {}, [el("summary", { text: "Encoder details" }), el("div.hs-grid", {}, [
    field("Keyframe every, frames", c.keyframe_frames, "Taken over the seconds when set."),
    field("Preset", c.preset),
    field("B frames", c.bframes, "Left empty, none."),
  ])]);
  if (f.keyframe_frames || f.preset || f.bframes) fine.open = true;
  const sound = el("div.hs-grid", {}, [field("Sound", c.audio), field("Bitrate, kb/s", c.audio_kbps), field("Channels", c.channels), field("Tracks", c.tracks)]);
  const node = el("div.hs-custom", {}, [el("h4.hs-sub", { text: "Picture" }), video, fine, el("h4.hs-sub", { text: "Sound" }), sound]);
  const sync = () => {
    const copy = c.codec.value === "copy";
    for (const k of ["size", "fit", "pad_color", "bitrate_kbps", "keyframe_s", "deinterlace", "encoder"]) c[k].closest(".field").hidden = copy;
    fine.hidden = copy;
    const sized = c.size.value !== "source";
    c.fit.closest(".field").hidden = copy || !sized;
    c.pad_color.closest(".field").hidden = copy || !sized || c.fit.value !== "pad";
    const quiet = c.audio.value === "copy";
    for (const k of ["audio_kbps", "channels", "tracks"]) c[k].closest(".field").hidden = quiet;
  };
  node.addEventListener("change", sync);
  sync();
  return {
    node,
    controls: c,
    read() {
      const out = {};
      for (const [k, input] of Object.entries(c)) out[k] = input.value;
      return wantFrom(out);
    },
  };
}

/**
 * The profile picker with the custom form under it. `value()` is the
 * channel's {profile, custom?}.
 */
export function convPicker(setup, channel, opts = {}) {
  const profiles = (setup && setup.profiles) || [];
  const rows = [
    ...profiles.filter((p) => p.builtin).map((p) => [p.id, p.name || p.id]),
    ...profiles.filter((p) => !p.builtin).map((p) => [p.id, `${p.name || p.id} (saved)`]),
  ];
  const current = (channel && channel.profile) || "copy";
  if (current !== "custom" && !rows.some(([id]) => id === current)) rows.push([current, `${current} (not on this station)`]);
  rows.push(["custom", "Custom settings for this channel"]);
  const pick = choice(rows, current, opts.label || "Profile");
  const words = el("p.hs-note.dim", { "aria-live": "polite" });
  const slot = el("div.hs-customslot");
  let custom = null;
  const sync = () => {
    const id = pick.value;
    if (id === "custom") {
      if (!custom) {
        // Custom starts from the channel's own settings, else the profile it
        // took when that one converts, else 720p H.264 with AAC sound.
        const was = profiles.find((p) => p.id === current);
        custom = customForm((channel && channel.custom) || (was && (was.video || was.audio) ? was : STARTER), setup && setup.encoders);
        slot.replaceChildren(custom.node);
      }
      slot.hidden = false;
      words.textContent = "Settings of this channel's own. To share them, save them as a profile under Profiles.";
    } else {
      slot.hidden = true;
      words.textContent = profileWords(profiles.find((p) => p.id === id));
    }
  };
  pick.addEventListener("change", sync);
  sync();
  return {
    node: el("div.hs-conv", {}, [field(opts.label || "Profile", pick), words, slot]),
    pick,
    get custom() { return custom; },
    value() {
      if (pick.value !== "custom") return { profile: pick.value };
      return { profile: "custom", custom: custom.read() };
    },
  };
}
