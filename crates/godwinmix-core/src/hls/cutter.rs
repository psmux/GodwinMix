//! Turning what `cmafmux` writes into init segments, segments and parts.
//!
//! The muxer's output, as measured on GStreamer 1.28 (see
//! `docs/explanation/hls-output.md`):
//!
//! * one buffer holding `ftyp` and `moov`, flagged `HEADER`: the init segment;
//! * per chunk, a buffer holding `moof` and the `mdat` header, flagged
//!   `HEADER`, and `DELTA_UNIT` unless it starts a fragment; its timestamp is
//!   the chunk's start and its duration the chunk's;
//! * then the samples, one buffer each, the last flagged `MARKER`.
//!
//! So a `moof` without `DELTA_UNIT` starts a segment, every `moof` starts a
//! part, and `MARKER` ends one. Boxes are recognised by their type rather
//! than by flags alone, so a muxer that flags differently fails loudly in the
//! tests rather than quietly writing a broken playlist.

use super::follow::Follow;
use super::ring::Part;
use super::track::Track;
use bytes::Bytes;
use std::sync::Arc;

/// What the cutter needs to know about one buffer.
#[derive(Debug, Clone, Copy, Default)]
pub struct Piece {
    pub header: bool,
    pub delta: bool,
    pub marker: bool,
    /// Running time of the buffer's first sample.
    pub running_ns: Option<u64>,
    pub duration_ns: Option<u64>,
    /// Wall clock at `running_ns`, in milliseconds since 1970.
    pub wall_ms: i64,
}

/// The wall clock at running time `at_ns`, given the wall clock and the
/// pipeline's running time now: now, less however long ago the pipeline
/// says that was.
///
/// A buffer can be ahead of the clock (a pipeline that took a second to
/// start, fed timestamps from before it did), and is then later than now
/// by as much. Kept signed, every buffer of a pipeline is on one line from
/// running time to wall clock, so a segment's time agrees with its length
/// and the picture's with the sound's. Clamped at now, as it once was, a
/// short fragment of sound got the moment it arrived while a long one of
/// picture did not, and on a slow machine their times drifted apart.
pub fn wall_ms(now_ms: i64, clock_ns: Option<u64>, at_ns: Option<u64>) -> i64 {
    match (clock_ns, at_ns) {
        (Some(now), Some(at)) => now_ms - (now as i64 - at as i64) / 1_000_000,
        _ => now_ms,
    }
}

pub struct Cutter {
    track: Arc<Track>,
    /// Every part starts a new decodable run: audio, where every sample is a
    /// keyframe.
    all_independent: bool,
    /// The muxer writes one `moof` per fragment, so the end of a part is the
    /// end of its segment and it can be closed at once.
    whole_fragments: bool,
    pending: Option<(Vec<u8>, u64, bool)>,
    /// Sound beside a picture: every fragment goes to [`Follow`], which
    /// cuts the sound where the picture is cut.
    follow: Option<Follow>,
}

impl Cutter {
    pub fn new(track: Arc<Track>, all_independent: bool, whole_fragments: bool) -> Cutter {
        Cutter { track, all_independent, whole_fragments, pending: None, follow: None }
    }

    /// A cutter for sound that follows the picture's segments.
    pub fn follower(follow: Follow, track: Arc<Track>) -> Cutter {
        Cutter { track, all_independent: true, whole_fragments: false, pending: None, follow: Some(follow) }
    }

    pub fn push(&mut self, data: &[u8], piece: Piece) {
        match (piece.header, box_type(data)) {
            (true, Some(b"ftyp")) => self.track.set_init(Bytes::copy_from_slice(data)),
            (true, Some(b"moof" | b"styp")) => {
                self.finish_part();
                let decode_time = (!piece.delta).then(|| super::boxes::decode_time(data)).flatten();
                match self.follow.as_mut() {
                    Some(f) => f.start(piece.running_ns.unwrap_or(0), piece.wall_ms, decode_time),
                    None if !piece.delta => {
                        self.track.begin(piece.running_ns.unwrap_or(0), piece.wall_ms, decode_time);
                    }
                    None => {}
                }
                let independent = !piece.delta || self.all_independent;
                let mut bytes = Vec::with_capacity(data.len() + 16 * 1024);
                bytes.extend_from_slice(data);
                self.pending = Some((bytes, piece.duration_ns.unwrap_or(0), independent));
            }
            _ => {
                if let Some((bytes, _, _)) = self.pending.as_mut() {
                    bytes.extend_from_slice(data);
                }
            }
        }
        if piece.marker {
            self.finish_part();
            if self.whole_fragments {
                self.track.close();
            }
        }
    }

    /// The stream ended: whatever is pending is the last part of the last
    /// segment.
    pub fn eos(&mut self) {
        self.finish_part();
        match self.follow.as_mut() {
            Some(f) => f.eos(),
            None => self.track.close(),
        }
    }

    fn finish_part(&mut self) {
        if let Some((mut bytes, duration_ns, independent)) = self.pending.take() {
            // Held for the whole window, so give back what the growing left
            // spare: a part is kept at its size, not its capacity.
            bytes.shrink_to_fit();
            match self.follow.as_mut() {
                Some(f) => f.push(Bytes::from(bytes), duration_ns),
                None => {
                    self.track.push_part(Part { bytes: Bytes::from(bytes), duration_ns, independent });
                }
            }
        }
    }
}

/// The four character type of the first box in `data`.
fn box_type(data: &[u8]) -> Option<&[u8; 4]> {
    data.get(4..8).and_then(|t| t.try_into().ok())
}

#[cfg(test)]
#[path = "cutter_tests.rs"]
mod tests;
