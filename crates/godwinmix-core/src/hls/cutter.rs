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
use bytes::{Bytes, BytesMut};
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
    pending: Option<(BytesMut, u64, bool)>,
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
                    self.track.begin(piece.running_ns.unwrap_or(0), piece.wall_ms);
                }
                let independent = !piece.delta || self.all_independent;
                let mut bytes = BytesMut::with_capacity(data.len() + 64 * 1024);
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
        if let Some((bytes, duration_ns, independent)) = self.pending.take() {
            self.track.push_part(Part { bytes: bytes.freeze(), duration_ns, independent });
        }
    }
}

/// The four character type of the first box in `data`.
fn box_type(data: &[u8]) -> Option<&[u8; 4]> {
    data.get(4..8).and_then(|t| t.try_into().ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::track::TrackKind;
    use crate::hls::HlsParams;

    fn boxed(kind: &[u8; 4], len: usize) -> Vec<u8> {
        let mut v = (len as u32).to_be_bytes().to_vec();
        v.extend_from_slice(kind);
        v.resize(len, 0);
        v
    }

    fn moof(delta: bool, at_ms: u64) -> Piece {
        Piece {
            header: true,
            delta,
            running_ns: Some(at_ms * 1_000_000),
            duration_ns: Some(333_333_333),
            ..Piece::default()
        }
    }

    const SAMPLE: Piece = Piece { header: false, delta: true, marker: false, running_ns: None, duration_ns: None, wall_ms: 0 };
    const LAST: Piece = Piece { marker: true, ..SAMPLE };

    fn track() -> Arc<Track> {
        Arc::new(Track::new("720p", TrackKind::Video, HlsParams::default(), 3000))
    }

    #[test]
    fn chunks_become_parts_and_fragments_become_segments() {
        let t = track();
        let mut c = Cutter::new(t.clone(), false, false);
        c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
        for f in 0..2u64 {
            for chunk in 0..6u64 {
                c.push(&boxed(b"moof", 100), moof(chunk > 0, 2000 * (f + 50) + chunk * 333));
                c.push(&[1; 500], SAMPLE);
                c.push(&[2; 500], LAST);
            }
        }
        let pos = t.position();
        assert_eq!(pos.complete, Some(50), "the first fragment closed when the second began");
        assert_eq!(pos.open, Some((51, 6)), "the second is still open: nothing said it ended");
        assert_eq!(t.segment(50).unwrap().iter().map(Bytes::len).sum::<usize>(), 6 * 1100);
        assert_eq!(t.part(51, 5).unwrap().len(), 1100);
        let view = t.view();
        assert_eq!(view.segments[0].parts.iter().filter(|(_, i)| *i).count(), 1, "only the first part holds a keyframe");
        c.eos();
        assert_eq!(t.position().complete, Some(51));
    }

    #[test]
    fn whole_fragments_close_at_their_marker() {
        let t = track();
        let mut c = Cutter::new(t.clone(), false, true);
        c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
        c.push(&boxed(b"moof", 100), moof(false, 0));
        c.push(&[1; 10], LAST);
        assert_eq!(t.position().complete, Some(0));
    }

    #[test]
    fn samples_before_the_first_moof_are_dropped() {
        let t = track();
        let mut c = Cutter::new(t.clone(), false, false);
        c.push(&[1; 10], LAST);
        c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
        assert_eq!(t.position(), Default::default());
    }

    #[test]
    fn audio_parts_are_all_independent() {
        let t = track();
        let mut c = Cutter::new(t.clone(), true, false);
        c.push(&boxed(b"ftyp", 40), Piece { header: true, ..Piece::default() });
        c.push(&boxed(b"moof", 100), moof(false, 0));
        c.push(&[1; 10], LAST);
        c.push(&boxed(b"moof", 100), moof(true, 333));
        c.push(&[1; 10], LAST);
        assert!(t.view().segments[0].parts.iter().all(|(_, i)| *i));
    }
}
