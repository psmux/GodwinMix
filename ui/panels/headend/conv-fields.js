// A conversion's custom settings, flat, as the form holds them, and the
// profile's {video, audio, ladder} made from them again. No DOM. The
// fields past size and bitrate are picture.js's.

import { extraFields, extrasInto } from "./picture.js";

const str = (v) => String(v ?? "").trim();

export const SIZES = [["source", "As it comes"], ["3840x2160", "3840×2160 (4K)"], ["1920x1080", "1920×1080 (1080p)"], ["1280x720", "1280×720 (720p)"], ["1024x576", "1024×576 (576p)"], ["854x480", "854×480 (480p)"], ["640x360", "640×360 (360p)"]];

/** Fields from a profile's video and audio wants, and its ladder. */
export function convFields(p) {
  const v = (p && p.video) || null;
  const a = (p && p.audio) || null;
  const f = { codec: v ? v.codec || "h264" : "copy", size: "source", fit: "pad", pad_color: "#000000", bitrate_kbps: "", keyframe_s: "2", keyframe_frames: "", deinterlace: "auto", preset: "", bframes: "", encoder: "auto", audio: "copy", audio_kbps: "128", channels: "", tracks: "first", ...extraFields(v), ladder: structuredClone((p && p.ladder) || []) };
  if (v) {
    if (v.width && v.height) f.size = `${v.width}x${v.height}`;
    for (const k of ["fit", "pad_color", "deinterlace", "preset", "encoder"]) if (v[k]) f[k] = v[k];
    if (!v.deinterlace) f.deinterlace = "off";
    if (v.bitrate_kbps) f.bitrate_kbps = String(v.bitrate_kbps);
    f.keyframe_s = v.keyframe_ms ? String(v.keyframe_ms / 1000) : "";
    if (v.keyframe_frames) f.keyframe_frames = String(v.keyframe_frames);
    if (v.bframes != null) f.bframes = String(v.bframes);
  }
  if (a) {
    f.audio = a.codec || "aac";
    if (a.bitrate_kbps) f.audio_kbps = String(a.bitrate_kbps);
    if (a.channels) f.channels = String(a.channels);
    if (a.tracks) f.tracks = a.tracks;
  }
  return f;
}

/** A profile's {video, audio, ladder} from the fields. Copy leaves that half out, and the ladder with it. */
export function wantFrom(f) {
  const out = {};
  const n = (v) => (str(v) !== "" && Number(v) >= 0 ? Math.round(Number(v)) : null);
  if (f.codec && f.codec !== "copy") {
    const v = { codec: f.codec };
    if (f.size && f.size !== "source") {
      const [w, h] = f.size.split("x").map(Number);
      Object.assign(v, { width: w, height: h, fit: f.fit || "pad" });
      if (v.fit === "pad" && /^#[0-9a-f]{6}$/i.test(f.pad_color || "") && f.pad_color.toLowerCase() !== "#000000") v.pad_color = f.pad_color.toLowerCase();
    }
    if (n(f.bitrate_kbps)) v.bitrate_kbps = n(f.bitrate_kbps);
    if (Number(f.keyframe_s) > 0) v.keyframe_ms = Math.round(Number(f.keyframe_s) * 1000);
    if (n(f.keyframe_frames)) v.keyframe_frames = n(f.keyframe_frames);
    if (f.deinterlace && f.deinterlace !== "off") v.deinterlace = f.deinterlace;
    if (str(f.preset)) v.preset = str(f.preset);
    if (n(f.bframes) !== null) v.bframes = n(f.bframes);
    if (f.encoder === "gpu" || f.encoder === "cpu") v.encoder = f.encoder;
    out.video = extrasInto(v, f);
    if (Array.isArray(f.ladder) && f.ladder.length) out.ladder = f.ladder;
  }
  if (f.audio && f.audio !== "copy") {
    const a = { codec: f.audio };
    if (n(f.audio_kbps)) a.bitrate_kbps = n(f.audio_kbps);
    if (n(f.channels)) a.channels = n(f.channels);
    if (f.tracks === "all") a.tracks = "all";
    out.audio = a;
  }
  return out;
}
