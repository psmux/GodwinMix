//! One MPEG-TS rendition's cutter: frames in, in the order the hub gave
//! them, whole segments into its [`TsTrack`] out.
//!
//! ```text
//!   Unit ─► queue ─► placed on the CMAF picture's ring ─► Muxer ─► segment bytes ─► TsTrack
//!                    (Ring::cut_at for a keyframe, Ring::place for sound alone)
//! ```
//!
//! A frame is muxed only once it is known which segment it is in, so a
//! segment starts with its own PAT and PMT and, for `main`, its keyframe
//! with the parameter sets. Frames wait here, never on a lock, until the
//! picture's muxer has cut far enough to say; for a window at most. One
//! muxer serves the cutter's life, so continuity counters and the clock
//! carry on from one segment to the next.

use super::mux::{Muxer, Unit, UnitKind};
use super::ring::TsTrack;
use crate::hls::ring::{Cut, Place};
use crate::hls::stream::Stream;
use std::collections::VecDeque;
use std::sync::{Arc, Weak};

const MS: u64 = 1_000_000;
/// Two times this close are the same frame: tags carry whole milliseconds.
const SAME_NS: u64 = 5 * MS;
/// Where a sound frame is placed: about its middle.
const HALF_FRAME_NS: u64 = 10 * MS;

struct Open {
    msn: u64,
    disc: u32,
    pdt_ms: i64,
    start_ns: u64,
    bytes: Vec<u8>,
}

pub struct TsCut {
    stream: Weak<Stream>,
    track: Arc<TsTrack>,
    /// A further sound alone, cut by its frames' times; else `main`, cut at
    /// the picture's keyframes.
    alone: bool,
    mux: Muxer,
    /// The tag time at zero running time on the CMAF pipeline.
    base: u32,
    min_ns: u64,
    hold_ms: u32,
    queue: VecDeque<Unit>,
    open: Option<Open>,
    last_len: usize,
}

impl TsCut {
    pub fn new(stream: &Arc<Stream>, track: Arc<TsTrack>, base: u32) -> TsCut {
        let p = stream.params;
        let alone = track.sound.is_some();
        let hold_ms = (p.window_s * 1000).max(4 * p.segment_ms);
        let stream = Arc::downgrade(stream);
        let min_ns = p.mux_durations().0;
        TsCut { stream, track, alone, mux: Muxer::default(), base, min_ns, hold_ms, queue: VecDeque::new(), open: None, last_len: 0 }
    }
}

impl Extend<Unit> for TsCut {
    fn extend<I: IntoIterator<Item = Unit>>(&mut self, units: I) {
        for u in units {
            self.push(u);
        }
    }
}

impl TsCut {
    /// Take one frame or configuration, and mux whatever can be placed.
    pub fn push(&mut self, u: Unit) {
        self.queue.push_back(u);
        self.drain();
    }

    fn drain(&mut self) {
        let Some(leader) = self.stream.upgrade().and_then(|s| s.leader()) else { return };
        while let Some(u) = self.queue.front() {
            if u.is_config() {
                let Some(u) = self.queue.pop_front() else { break };
                self.mux.unit(&u, &mut Vec::new());
                continue;
            }
            let Some(at) = self.running_ns(u) else {
                self.queue.pop_front();
                continue;
            };
            let video_key = u.kind == UnitKind::Video && u.key;
            let starts = match (self.alone, video_key) {
                (false, true) => match leader.cut_at(at, self.min_ns, SAME_NS) {
                    Cut::Starts { msn, disc, pdt_ms } => Some(Some((msn, disc, pdt_ms))),
                    Cut::Inside => Some(None),
                    Cut::Drop => None,
                    Cut::Wait if self.held() <= self.hold_ms => break,
                    Cut::Wait => None,
                },
                (false, false) => Some(None),
                (true, _) => match leader.place(at + HALF_FRAME_NS, self.min_ns) {
                    Place::In(msn) if self.open.as_ref().is_some_and(|o| o.msn >= msn) => Some(None),
                    Place::In(msn) => match leader.meta(msn) {
                        Some((disc, pdt_ms)) => Some(Some((msn, disc, pdt_ms))),
                        None if self.held() <= self.hold_ms => break,
                        None => None,
                    },
                    Place::Wait if self.held() <= self.hold_ms => break,
                    Place::Wait | Place::Drop => None,
                },
            };
            let Some(u) = self.queue.pop_front() else { break };
            match starts {
                Some(Some((msn, disc, pdt_ms))) => self.begin(msn, disc, pdt_ms, at, &u),
                Some(None) => {}
                None => continue,
            }
            if let Some(open) = self.open.as_mut() {
                self.mux.unit(&u, &mut open.bytes);
            }
        }
    }

    /// Close the open segment, as long as it ran to `at_ns`, and open `msn`.
    fn begin(&mut self, msn: u64, disc: u32, pdt_ms: i64, at_ns: u64, first: &Unit) {
        if let Some(o) = self.open.take() {
            self.last_len = o.bytes.len();
            let duration_ns = at_ns.saturating_sub(o.start_ns);
            self.track.push(o.msn, o.disc, o.pdt_ms, o.start_ns, duration_ns, o.bytes.into());
        }
        let mut bytes = Vec::with_capacity(self.last_len + self.last_len / 8);
        if self.alone {
            self.mux.tables(first.ms, &mut bytes);
        }
        self.open = Some(Open { msn, disc, pdt_ms, start_ns: at_ns, bytes });
    }

    /// A frame's presentation time on the CMAF pipeline's running time.
    fn running_ns(&self, u: &Unit) -> Option<u64> {
        let ms = i64::from(u.ms) - i64::from(self.base) + i64::from(u.cts_ms.max(0));
        (ms >= 0).then(|| ms as u64 * MS)
    }

    fn held(&self) -> u32 {
        match (self.queue.front(), self.queue.back()) {
            (Some(a), Some(b)) => b.ms.wrapping_sub(a.ms),
            _ => 0,
        }
    }
}

#[cfg(test)]
#[path = "cut_tests.rs"]
mod tests;
