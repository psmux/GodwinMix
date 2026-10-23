//! The video half of a node's description: how a scale lays the picture in,
//! and an encoder's properties with the request's own preset, B frames and
//! keyframe interval in the encoder's units.

use godwinmix_core::catalogue::apply::{cpu_count, Vars};
use godwinmix_protocol::rendition::{Deinterlace, Fit, StreamInfo, VideoShape};
use godwinmix_render::container::video_slug;
use godwinmix_render::NodeKind;
use serde_json::{json, Value};

use super::machine::Machine;
use super::spec::{fps_pair, resolve};

/// A scale node: size and rate, `fit` (`pad`, `crop`, `stretch`) with the
/// bars' colour as `0xAARRGGBB` the way GStreamer's scalers take it,
/// `deinterlace` (`off`, `on`, `auto`), and `rate` when the frame rate
/// changes, so a scale to the source's own rate never drops or repeats one.
pub fn scale(kind: &NodeKind, input: &str, info: &StreamInfo) -> Value {
    let NodeKind::Scale { width, height, fps, fit, pad_color, deinterlace, .. } = kind else { return Value::Null };
    let fit = match fit {
        Fit::Pad => "pad",
        Fit::Crop => "crop",
        Fit::Stretch => "stretch",
    };
    let deinterlace = match deinterlace {
        Deinterlace::Off => "off",
        Deinterlace::On => "on",
        Deinterlace::Auto => "auto",
    };
    let rate = info.video.is_none_or(|v| v.fps.as_f64() != fps.as_f64());
    json!({
        "kind": "scale", "input": input, "width": width, "height": height, "fps": fps_pair(*fps),
        "fit": fit, "border": argb(pad_color.as_deref()), "deinterlace": deinterlace, "rate": rate,
    })
}

/// `#rrggbb` as an opaque `0xffrrggbb`; black when it does not read.
fn argb(color: Option<&str>) -> u64 {
    let hex = color.unwrap_or("").trim_start_matches('#').trim_start_matches("0x");
    0xff00_0000 | u64::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6).unwrap_or(0)
}

/// Whether an encoder of this acceleration takes `preset` as written:
/// NVENC's `p1` to `p7`, x264 and x265's `ultrafast` to `placebo`. Any other
/// family takes a preset in its own words, so it is passed on as it is.
fn speaks(accel: &str, preset: &str) -> bool {
    const X264: &[&str] = &["ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow", "placebo"];
    let nvenc = preset.len() == 2 && preset.starts_with('p') && preset[1..].parse::<u8>().is_ok_and(|n| (1..=7).contains(&n));
    match accel {
        "nvidia" => nvenc,
        "software" => X264.contains(&preset),
        _ => !nvenc && !X264.contains(&preset),
    }
}

pub fn video_encoder(id: &str, shape: &VideoShape, keyframe_ms: u32, preset: Option<&str>, bframes: Option<u8>, machine: &Machine) -> Option<Value> {
    let e = machine.encoder(id)?;
    let fps = shape.fps.as_f64().max(1.0);
    let frames = ((f64::from(keyframe_ms) * fps / 1000.0).round() as i64).max(1);
    let vars = Vars {
        video_bitrate_kbps: i64::from(shape.bitrate_kbps),
        keyframe_frames: frames,
        keyframe_secs: (i64::from(keyframe_ms) / 1000).max(1),
        fps: fps.round() as i64,
        cpu_count: cpu_count(),
        ..Vars::default()
    };
    let mut props = resolve(&e.properties, &vars);
    if let Some(kf) = &e.keyframe {
        let value = if kf.unit == "seconds" { vars.keyframe_secs } else { frames };
        props.insert(kf.property.clone(), json!(value));
    }
    // The request's own B frames, and its preset when the preset is one this
    // encoder knows, go where the catalogue already sets them, under this
    // element's name for them. A preset is one family's word: NVENC's `p1`
    // means nothing to x264, which then fell back to a CPU encode at its
    // own preset with a warning, so one for another family is left out.
    let preset = preset.filter(|p| speaks(&e.accel, p));
    for (name, value) in [("preset", preset.map(|p| json!(p))), ("speed-preset", preset.map(|p| json!(p))), ("bframes", bframes.map(|b| json!(b))), ("b-frames", bframes.map(|b| json!(b)))] {
        if let (Some(v), true) = (value, props.contains_key(name)) {
            props.insert(name.to_string(), v);
        }
    }
    let mut v = json!({
        "encoder": id, "element": e.encoder, "props": props, "hardware": e.accel != "software", "accel": e.accel,
        "codec": video_slug(shape.codec), "width": shape.width, "height": shape.height,
        "fps": fps_pair(shape.fps), "bitrate_kbps": shape.bitrate_kbps, "keyframe_ms": keyframe_ms, "keyframe_frames": frames,
    });
    if let Some(p) = &e.parser {
        v["parser"] = json!(p);
    }
    if let Some(p) = &e.profile {
        v["profile"] = json!(p);
    }
    Some(v)
}

#[cfg(test)]
mod tests {
    use super::speaks;

    #[test]
    fn a_preset_reaches_only_the_family_that_speaks_it() {
        assert!(speaks("nvidia", "p1") && !speaks("nvidia", "veryfast"));
        assert!(speaks("software", "veryfast") && !speaks("software", "p1"));
        assert!(!speaks("videotoolbox", "p1") && speaks("qsv", "4"));
    }
}
