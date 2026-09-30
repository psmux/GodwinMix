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

pub struct Cutter {
    track: Arc<Track>,
    /// Every part starts a new decodable run: audio, where every sample is a
    /// keyframe.
    all_independent: bool,
    /// The muxer writes one `moof` per fragment, so the end of a part is the
    /// end of its segment and it can be closed at once.
    whole_fragments: bool,
    pending: Option<(Vec<u8>, u64, bool)>,
}

impl Cutter {
    pub fn new(track: Arc<Track>, all_independent: bool, whole_fragments: bool) -> Cutter {
        Cutter { track, all_independent, whole_fragments, pending: None }
    }

    pub fn push(&mut self, data: &[u8], piece: Piece) {
        match (piece.header, box_type(data)) {
            (true, Some(b"ftyp")) => self.track.set_init(Bytes::copy_from_slice(data)),
            (true, Some(b"moof" | b"styp")) => {
                self.finish_part();
                if !piece.delta {
                    let decode_time = super::boxes::decode_time(data);
                    self.track.begin(piece.running_ns.unwrap_or(0), piece.wall_ms, decode_time);
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
        self.track.close();
    }

    fn finish_part(&mut self) {
        if let Some((mut bytes, duration_ns, independent)) = self.pending.take() {
            // Held for the whole window, so give back what the growing left
            // spare: a part is kept at its size, not its capacity.
            bytes.shrink_to_fit();
            self.track.push_part(Part { bytes: Bytes::from(bytes), duration_ns, independent });
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
