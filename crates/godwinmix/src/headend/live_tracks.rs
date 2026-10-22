//! Tracks in Flussonic's words, from what an input reports and from what an
//! output's master playlist says.

use godwinmix_flussonic::api::MediaTrack;
use godwinmix_flussonic::facade::access::{Kind, Tracks};
use serde_json::Value;

/// GodwinMix's codec names as Flussonic writes them.
pub fn codec(name: &str) -> String {
    match name.to_ascii_lowercase().as_str() {
        "h265" | "hevc" => "hevc".into(),
        "mpeg2" | "mpeg-2" | "mpeg2video" | "mp2v" => "mp2v".into(),
        "mpeg audio" | "mp2" | "mpga" | "mp2a" => "mp2a".into(),
        "e-ac-3" | "eac3" => "eac3".into(),
        "ac-3" | "ac3" => "ac3".into(),
        other => other.into(),
    }
}

fn num(v: &Value) -> Option<u32> {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).filter(|n| *n > 0)
}

/// An input's tracks. `tracks` (one entry per carried track, with its PID)
/// when the input reports it, else the one video and one sound it has
/// always reported.
pub fn input(v: &Value) -> Vec<MediaTrack> {
    if let Some(list) = v["tracks"].as_array() {
        let mut n = (0, 0);
        return list.iter().map(|t| {
            let video = t["kind"] == "video";
            let id = if video { n.0 += 1; format!("v{}", n.0) } else { n.1 += 1; format!("a{}", n.1) };
            MediaTrack {
                content: if video { "video" } else { "audio" },
                track_id: id,
                codec: codec(t["codec"].as_str().unwrap_or_default()),
                width: num(&t["width"]),
                height: num(&t["height"]),
                channels: num(&t["channels"]),
                sample_rate: num(&t["sample_rate"]),
                language: t["lang"].as_str().map(String::from),
                pid: t["pid"].as_u64().and_then(|p| u16::try_from(p).ok()),
                ..Default::default()
            }
        }).collect();
    }
    let mut out = Vec::new();
    if let Some(c) = v["video_codec"].as_str().filter(|c| !c.is_empty()) {
        let fps = v["fps"].as_f64().filter(|f| *f > 0.0).map(|f| (f * 100.0).round() / 100.0);
        let gop = match (v["keyframe_ms"].as_f64(), fps) {
            (Some(ms), Some(f)) => Some((ms * f / 1000.0).round() as u32),
            _ => None,
        };
        out.push(MediaTrack { content: "video", track_id: "v1".into(), codec: codec(c), width: num(&v["width"]), height: num(&v["height"]), fps, last_gop: gop, ..Default::default() });
    }
    if let Some(c) = v["audio_codec"].as_str().filter(|c| !c.is_empty()) {
        out.push(MediaTrack { content: "audio", track_id: "a1".into(), codec: codec(c), channels: num(&v["audio_channels"]), ..Default::default() });
    }
    out
}

/// `avc1.64001f` as codec, profile and level.
fn rfc6381(codecs: &str) -> (String, Option<String>, Option<String>) {
    let (family, rest) = codecs.split_once('.').unwrap_or((codecs, ""));
    match family {
        "avc1" | "avc3" if rest.len() >= 6 => {
            let profile = match &rest[0..2] {
                "42" => "Baseline",
                "4d" | "4D" => "Main",
                "64" => "High",
                _ => "",
            };
            let level = u8::from_str_radix(&rest[4..6], 16).ok().map(|l| format!("{}.{}", l / 10, l % 10));
            ("h264".into(), (!profile.is_empty()).then(|| profile.to_string()), level)
        }
        "avc1" | "avc3" => ("h264".into(), None, None),
        "hvc1" | "hev1" => ("hevc".into(), None, None),
        "mp4a" if rest == "40.34" => ("mp3".into(), None, None),
        "mp4a" => ("aac".into(), None, None),
        "ac-3" => ("ac3".into(), None, None),
        "ec-3" => ("eac3".into(), None, None),
        other => (other.to_string(), None, None),
    }
}

/// An output's tracks, from its master playlist, with the bitrate it sends.
pub fn output(t: &Tracks, kbps: u32) -> Vec<MediaTrack> {
    t.tracks.iter().map(|x| {
        let (codec, profile, level) = rfc6381(&x.codecs);
        let video = x.kind == Kind::Video;
        let id = if video { format!("v{}", x.number) } else { format!("a{}", x.number) };
        MediaTrack {
            content: if video { "video" } else { "audio" },
            track_id: id,
            codec,
            width: (x.width > 0).then_some(x.width),
            height: (x.height > 0).then_some(x.height),
            fps: x.fps.map(|f| (f * 100.0).round() / 100.0),
            bitrate: video.then_some(kbps).filter(|k| *k > 0).or_else(|| (x.bandwidth > 0).then(|| (x.bandwidth / 1000) as u32)),
            profile,
            level,
            channels: x.channels,
            language: x.lang.clone(),
            ..Default::default()
        }
    }).collect()
}
