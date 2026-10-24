//! The sound tracks after the first that a headend input reports in
//! `tracks`, as the planner's further tracks.

use godwinmix_protocol::rendition::{AudioShape, SoundTrack};
use serde_json::Value;

use super::transcode::audio_codec;

/// Each `{track, kind: "audio", codec, channels, sample_rate, kbps, lang}`
/// numbered 1 and up, in track order. Track 0 is the stream's `audio`.
pub fn more_audio(tracks: &Value) -> Vec<SoundTrack> {
    let n = |t: &Value, k: &str| t.get(k).and_then(Value::as_u64).unwrap_or(0);
    let mut out: Vec<SoundTrack> = tracks
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["kind"] == "audio" && n(t, "track") > 0 && n(t, "channels") > 0)
        .map(|t| SoundTrack {
            track: u8::try_from(n(t, "track")).unwrap_or(u8::MAX),
            shape: AudioShape {
                codec: audio_codec(t["codec"].as_str().unwrap_or_default()),
                channels: u8::try_from(n(t, "channels")).unwrap_or(2),
                sample_rate: u32::try_from(n(t, "sample_rate")).unwrap_or(48_000),
                bitrate_kbps: u32::try_from(n(t, "kbps")).unwrap_or(0),
            },
            lang: t["lang"].as_str().map(str::to_string),
        })
        .collect();
    out.sort_by_key(|t| t.track);
    out
}
