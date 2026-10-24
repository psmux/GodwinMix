//! Flussonic's names for codecs and frame rates, as a rendition spells them.

use serde_json::{json, Value};

/// Flussonic's codec names as a rendition spells them.
pub fn video_codec(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "h264" | "avc" => "h264",
        "hevc" | "h265" => "h265",
        "av1" => "av1",
        "vp9" => "vp9",
        "mp2v" | "mpeg2" | "mpeg2video" => "mpeg2",
        _ => return None,
    })
}

pub fn audio_codec(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "aac" => "aac",
        "opus" => "opus",
        "mp3" => "mp3",
        "ac3" => "ac3",
        _ => return None,
    })
}

/// `fps=29.97` as the exact fraction a rendition takes.
pub fn fps_of(fps: f64) -> Value {
    let ntsc = [(23.976, 24_000), (29.97, 30_000), (59.94, 60_000)];
    match ntsc.iter().find(|(f, _)| (fps - f).abs() < 0.01) {
        Some((_, num)) => json!({"num": num, "den": 1001}),
        None => json!({"num": fps.round() as u32, "den": 1}),
    }
}
