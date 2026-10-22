//! Sound tracks numbered for a show that converts every one of them.
//!
//! Every audio stream of the program is carried, numbered in the order the
//! demuxer offered them, which is PMT order: the first is track 0, the one
//! every reader of the hub has always had, and the others 1, 2, 3. When one
//! goes the rest close up, so a sender restarting with a new layout (its new
//! streams added, then its old ones removed) ends with its first sound on
//! track 0 again.

use std::sync::atomic::Ordering;

use gstreamer as gst;
use gstreamer::prelude::*;

use super::stats::TrackInfo;
use super::streams::{Meta, Slot, Stream};

/// Number the audio streams that pass, in the order they came.
pub fn renumber(all: &[Stream]) {
    let audio = all.iter().filter(|s| s.slot == Slot::Audio && s.gate.on.load(Ordering::Relaxed));
    for (n, s) in audio.enumerate() {
        s.gate.track.store(u8::try_from(n).unwrap_or(u8::MAX), Ordering::Relaxed);
    }
}

/// What is carried, video first, then sound by track, with its PID and
/// language where the demuxer said them.
pub fn tracks(all: &[Stream]) -> Vec<TrackInfo> {
    let mut out: Vec<TrackInfo> = all
        .iter()
        .filter(|s| s.slot != Slot::None && s.gate.on.load(Ordering::Relaxed))
        .map(|s| TrackInfo {
            track: s.gate.track.load(Ordering::Relaxed),
            kind: if s.slot == Slot::Video { "video" } else { "audio" },
            pid: s.meta.pid,
            lang: s.meta.lang.clone(),
        })
        .collect();
    out.sort_by_key(|t| (t.kind != "video", t.track));
    out
}

impl super::streams::Streams {
    /// Every stream carried now, for the input's stats.
    pub fn tracks(&self) -> Vec<TrackInfo> {
        tracks(&self.lock())
    }
}

/// The PID and language of a demuxer's pad. `tsdemux` ends a stream's id
/// with its PID in hex and puts the PMT's language descriptor in the
/// stream's tags as an ISO 639 code.
pub fn meta(pad: &gst::Pad) -> Meta {
    let Some(stream) = pad.stream() else { return Meta::default() };
    let pid = stream.stream_id().and_then(|id| id.rsplit(['/', ':']).next().and_then(|h| u16::from_str_radix(h, 16).ok()));
    let lang = stream.tags().and_then(|t| t.get::<gst::tags::LanguageCode>().map(|v| three_letters(&v.get().to_ascii_lowercase())));
    Meta { pid, lang }
}

/// The PMT's own three letter code back from the two letter one GStreamer
/// turns it into, for the languages a broadcast carries; any other is left
/// as GStreamer gave it.
fn three_letters(code: &str) -> String {
    const CODES: &[(&str, &str)] = &[
        ("en", "eng"), ("es", "spa"), ("fr", "fra"), ("de", "deu"), ("it", "ita"), ("pt", "por"), ("ru", "rus"),
        ("zh", "zho"), ("ja", "jpn"), ("ko", "kor"), ("ar", "ara"), ("hi", "hin"), ("vi", "vie"), ("tl", "tgl"),
        ("ht", "hat"), ("pl", "pol"), ("el", "ell"), ("he", "heb"), ("nl", "nld"), ("sv", "swe"), ("uk", "ukr"),
    ];
    CODES.iter().find(|(two, _)| *two == code).map_or(code, |(_, three)| three).to_string()
}
