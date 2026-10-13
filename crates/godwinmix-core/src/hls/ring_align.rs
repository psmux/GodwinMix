//! Keeping the tracks of one stream in step, so segment N is the same
//! moment in the same discontinuity on the picture and on the sound. RFC
//! 8216 asks for it of renditions, and hls.js lines its sound up by it: when
//! the sound's playlist was one discontinuity ahead of the picture's, a new
//! viewer loaded nothing.
//!
//! Two things keep them in step. A break (the feed stopped and started
//! again) is marked on every track at once, with the stream's own count
//! rather than each track's count of init segments. And the sound is cut
//! where the picture is cut, by [`Ring::place`] on the picture's ring, so
//! both have as many segments of the same length and their windows cover
//! the same time.

use super::Ring;

/// Where a follower's fragment goes, by the leader's segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Place {
    /// In segment `msn`.
    In(u64),
    /// Before anything the leader holds in this discontinuity: let it go.
    Drop,
    /// The leader has not cut far enough to say yet.
    Wait,
}

impl Ring {
    /// The feed stopped and starts again: give the next segment
    /// discontinuity `disc` and number `last + 1`, every track the same.
    ///
    /// `last` is the lowest newest segment of the stream's tracks. A track
    /// with a segment past it holds what no other track has (the sound of a
    /// segment the picture never finished), so that goes, and the numbers
    /// stay one apart with no gap, which a player relies on.
    pub fn mark_break(&mut self, disc: u32, last: u64) {
        while self.segments.back().is_some_and(|s| s.msn > last) {
            self.segments.pop_back();
        }
        self.close();
        self.disc = disc;
        self.resume = Some(last + 1);
        self.version += 1;
    }

    /// Start in discontinuity `disc`: a track added to a stream that has
    /// already had a break.
    pub fn join(&mut self, disc: u32) {
        self.disc = disc;
    }

    /// The discontinuity the next segment is in.
    pub fn disc(&self) -> u32 {
        self.disc
    }

    /// The newest segment's number, whole or open.
    pub fn newest(&self) -> Option<u64> {
        self.segments.back().map(|s| s.msn)
    }

    /// Whether a segment has begun since the last break, or ever.
    pub fn cut_since_break(&self) -> bool {
        self.segments.back().is_some_and(|s| s.disc == self.disc)
    }

    /// The number the segment being begun gets: where a break or a leader
    /// said, else one after the last, else placed by its start time. A
    /// playlist numbers its segments one apart, so only the first is placed
    /// by time; after that a late keyframe makes a long segment, never a gap.
    pub(super) fn next_msn(&mut self, start_ns: u64, segment_ns: u64) -> u64 {
        match (self.resume.take(), self.segments.back()) {
            (Some(msn), _) => msn,
            (None, Some(last)) => last.msn + 1,
            (None, None) => (start_ns + segment_ns / 2) / segment_ns.max(1),
        }
    }

    /// Begin the segment a leader numbered `msn`. None when the open one
    /// already is that segment or a later one. Past the first segment the
    /// number is one after the last whatever `msn` says, so a segment of
    /// the picture with no sound at all costs the sound one number of
    /// alignment and never a gap.
    pub fn begin_at(&mut self, msn: u64, start_ns: u64, pdt_ms: i64) -> Option<u64> {
        if self.segments.back().is_some_and(|last| msn <= last.msn) {
            return None;
        }
        if self.segments.is_empty() && self.resume.is_none() {
            self.resume = Some(msn);
        }
        self.begin(start_ns, 1, pdt_ms)
    }

    /// Where a follower's fragment whose middle is at `at_ns` of running
    /// time goes, on this, the leader's ring. `min_ns` is the shortest a
    /// segment of the leader can be: its muxer cuts at the first keyframe
    /// at least that long after a segment starts.
    pub fn place(&self, at_ns: u64, min_ns: u64) -> Place {
        let now: Vec<_> = self.segments.iter().filter(|s| s.disc == self.disc).collect();
        let (Some(first), Some(newest)) = (now.first(), now.last()) else { return Place::Wait };
        if at_ns < first.start_ns {
            return Place::Drop;
        }
        let Some(s) = now.iter().rev().find(|s| s.start_ns <= at_ns) else { return Place::Drop };
        if s.msn != newest.msn {
            return Place::In(s.msn);
        }
        if !s.complete {
            return if at_ns < s.start_ns + min_ns { Place::In(s.msn) } else { Place::Wait };
        }
        let end = s.start_ns + s.duration_ns();
        if at_ns < end {
            Place::In(s.msn)
        } else if at_ns < end + min_ns {
            Place::In(s.msn + 1)
        } else {
            Place::Wait
        }
    }
}
