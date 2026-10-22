//! Which of a program's streams are carried, and as which track.
//!
//! The first video stream the PMT lists is the video. The first sound
//! stream is track 0 and, when the show carries every track, the others are
//! 1, 2, 3 in PMT order. Every stream left out is named, the way the
//! GStreamer path names what it leaves out.

use gmx_udp::ts::tables::{descriptors, Stream};

use super::es::Codec;
use crate::direct::input::spec::Carry;
use crate::direct::input::stats::TrackInfo;

/// One stream the PMT lists, and what becomes of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pick {
    pub stream: Stream,
    /// The codec and track it is carried as, or `None` when left out.
    pub carried: Option<(Codec, u8)>,
    /// Why it is left out, while it is.
    pub note: Option<String>,
}

/// Decide every stream of one PMT.
pub fn pick(streams: &[Stream], carry: Carry) -> Vec<Pick> {
    let (mut video, mut sound) = (false, 0u8);
    streams
        .iter()
        .map(|s| {
            let codec = Codec::of(s);
            let (carried, note) = match codec {
                Some(Codec::Mpeg2) if !carry.mpeg2 => (None, Some(format!("{}, which a direct show does not carry", s.kind()))),
                Some(c) if c.video() && video => (None, Some(format!("a second video stream ({}); the first is taken", s.kind()))),
                Some(c) if c.video() => {
                    video = true;
                    (Some((c, 0)), None)
                }
                Some(_) if sound > 0 && !carry.tracks => (None, Some(format!("a second audio stream ({}); the first is taken", s.kind()))),
                Some(c) => {
                    sound += 1;
                    (Some((c, sound - 1)), None)
                }
                None => (None, Some(format!("{}, which a direct show does not carry", s.kind()))),
            };
            Pick { stream: s.clone(), carried, note }
        })
        .collect()
}

/// What `InputStats.tracks` says of the carried streams: video first, then
/// sound by track.
pub fn infos(picks: &[Pick]) -> Vec<TrackInfo> {
    let mut out: Vec<TrackInfo> = picks
        .iter()
        .filter_map(|p| {
            let (codec, track) = p.carried?;
            let kind = if codec.video() { "video" } else { "audio" };
            Some(TrackInfo { track, kind, pid: Some(p.stream.pid), lang: lang(&p.stream) })
        })
        .collect();
    out.sort_by_key(|t| (t.kind != "video", t.track));
    out
}

/// The ISO 639 language: descriptor 0x0A, or the language of an ATSC AC-3
/// audio descriptor (A/52 annex A, tag 0x81). Three lower case letters.
pub fn lang(s: &Stream) -> Option<String> {
    let code = s.language().or_else(|| descriptors(&s.info).find(|(t, _)| *t == 0x81).and_then(|(_, d)| atsc_ac3(d)))?;
    let code = code.trim().to_ascii_lowercase();
    (code.len() == 3 && code.bytes().all(|b| b.is_ascii_lowercase())).then_some(code)
}

/// The `language` field near the end of an AC-3 audio descriptor, after
/// the fields of variable length before it.
fn atsc_ac3(d: &[u8]) -> Option<String> {
    let channels = (d.get(2)? >> 1) & 0x0F;
    // langcod, and langcod2 for a dual mono (1+1) stream, then the main or
    // associated service byte.
    let mut at = 4 + usize::from(channels == 0) + 1;
    let textlen = usize::from(d.get(at)? >> 1);
    at += 1 + textlen;
    let flags = *d.get(at)?;
    if flags & 0x80 == 0 {
        return None;
    }
    d.get(at + 1..at + 4).map(|l| String::from_utf8_lossy(l).into_owned())
}

#[cfg(test)]
#[path = "tracks_tests.rs"]
mod tests;
