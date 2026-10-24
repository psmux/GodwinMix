//! A track's side of keeping a stream's tracks in step (`ring_align.rs`):
//! each call takes the ring's lock for a few comparisons and never waits.

use super::super::ring::{Cut, Place};
use super::Track;

impl Track {
    /// Mark a break: see `Ring::mark_break`.
    pub fn mark_break(&self, disc: u32, last: u64) {
        self.change(|r| r.mark_break(disc, last));
    }

    /// Start in the stream's discontinuity, for a track added after a break.
    pub fn join(&self, disc: u32) {
        self.ring.lock().join(disc);
    }

    pub fn disc(&self) -> u32 {
        self.ring.lock().disc()
    }

    pub fn newest(&self) -> Option<u64> {
        self.ring.lock().newest()
    }

    /// Whether a segment has begun since the last break, or ever.
    pub fn cut_since_break(&self) -> bool {
        self.ring.lock().cut_since_break()
    }

    /// Begin the segment the leader numbered `msn`, closing the one before.
    /// None when the open segment already is that one.
    pub fn begin_at(&self, msn: u64, start_ns: u64, pdt_ms: i64, decode_time: Option<u64>) -> Option<u64> {
        self.change(|r| {
            let begun = r.begin_at(msn, start_ns, pdt_ms);
            if begun.is_some() {
                r.stamp(decode_time);
            }
            begun
        })
    }

    /// Where a follower's fragment goes: see `Ring::place`.
    pub fn place(&self, at_ns: u64, min_ns: u64) -> Place {
        self.ring.lock().place(at_ns, min_ns)
    }

    /// What a keyframe at `at_ns` is to the TS cutter: see `Ring::cut_at`.
    pub fn cut_at(&self, at_ns: u64, min_ns: u64, tol_ns: u64) -> Cut {
        self.ring.lock().cut_at(at_ns, min_ns, tol_ns)
    }

    /// Segment `msn`'s discontinuity and wall clock: see `Ring::meta`.
    pub fn meta(&self, msn: u64) -> Option<(u32, i64)> {
        self.ring.lock().meta(msn)
    }
}
