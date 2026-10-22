//! The sound tracks after the first, measured, and MPEG-2 video read off
//! its keyframes. Only a headend input carries either (`MediaTag::track`,
//! `crate::mp2v`); for every other stream this is an empty list and a
//! check of one byte.

use std::time::Instant;

use serde_json::{json, Value};

use crate::codec::{self, Audio, Video};
use crate::media_tag::{MediaTag, TagKind};

/// One further sound track.
struct Track {
    track: u8,
    audio: Audio,
    bytes: u64,
    kbps: u32,
    since: Instant,
}

/// Every sound track numbered 1 and up, in track order.
#[derive(Default)]
pub struct Tracks(Vec<Track>);

impl Tracks {
    /// Count a tag of a track after the first. True when the track is new
    /// or changed codec, which is worth an event.
    pub fn record(&mut self, tag: &MediaTag, now: Instant) -> bool {
        if tag.kind != TagKind::Audio {
            return false;
        }
        let at = match self.0.iter().position(|t| t.track == tag.track) {
            Some(at) => at,
            None => {
                let at = self.0.iter().position(|t| t.track > tag.track).unwrap_or(self.0.len());
                let audio = Audio { codec: String::new(), channels: 0, sample_rate: 0 };
                self.0.insert(at, Track { track: tag.track, audio, bytes: 0, kbps: 0, since: now });
                at
            }
        };
        let t = &mut self.0[at];
        let mut news = false;
        let headerless = tag.payload.first().is_some_and(|b| b >> 4 != 10);
        if tag.sequence_header || (headerless && t.audio.codec != codec::audio_codec(&tag.payload)) || t.audio.codec.is_empty() {
            let read = codec::read_audio(tag);
            news = read != t.audio;
            t.audio = read;
        }
        if !tag.sequence_header {
            t.bytes += tag.payload.len() as u64;
        }
        let secs = now.duration_since(t.since).as_secs_f64();
        if secs >= 1.0 {
            t.kbps = (t.bytes as f64 * 8.0 / 1000.0 / secs).round() as u32;
            (t.bytes, t.since) = (0, now);
        }
        news
    }

    /// `{track, kind, codec, channels, sample_rate, kbps}` for each.
    pub fn describe(&self, quiet: bool) -> Vec<Value> {
        let rate = |k: u32| if quiet { 0 } else { k };
        self.0
            .iter()
            .map(|t| json!({"track": t.track, "kind": "audio", "codec": t.audio.codec, "channels": t.audio.channels, "sample_rate": t.audio.sample_rate, "kbps": rate(t.kbps)}))
            .collect()
    }
}

/// What an MPEG-2 keyframe says: the codec and size, the exact frame rate,
/// and whether it is progressive. `None` for any other tag.
pub fn mpeg2(tag: &MediaTag) -> Option<(Video, f64, bool)> {
    if tag.kind != TagKind::Video || !tag.keyframe || !crate::mp2v::is(&tag.payload) {
        return None;
    }
    let s = crate::mp2v::sequence(tag.payload.get(crate::mp2v::PREFIX..)?)?;
    let fps = f64::from(s.fps.0) / f64::from(s.fps.1.max(1));
    Some((Video { codec: "mpeg2".into(), width: s.width, height: s.height }, fps, s.progressive))
}
