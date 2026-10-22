//! Where the MPEG-TS segments are cut: on the picture's ring, at the same
//! keyframes `cmafmux` cut the CMAF segments at, with the same numbers.
//!
//! The TS cutter sees each keyframe before the muxer has cut there, so it
//! asks the ring and waits when the ring cannot say yet. Without parts a
//! CMAF segment reaches the ring only once it is whole, which is the moment
//! the keyframe after it reached the muxer; so a keyframe at the end of the
//! newest whole segment starts the next one, and a keyframe past that waits
//! until the ring has more.

use super::{Ring, Segment};

/// What a keyframe at some running time is to the TS cutter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cut {
    /// It starts segment `msn`, in discontinuity `disc`, at wall clock `pdt_ms`.
    Starts { msn: u64, disc: u32, pdt_ms: i64 },
    /// It is inside a segment that started earlier.
    Inside,
    /// It is before anything the ring holds since its last break.
    Drop,
    /// The ring has not got that far.
    Wait,
}

impl Ring {
    /// What a keyframe at `at_ns` is. `min_ns` is the shortest a segment can
    /// be and `tol_ns` how far apart two times may be and still be the same
    /// frame (the times are whole milliseconds).
    pub fn cut_at(&self, at_ns: u64, min_ns: u64, tol_ns: u64) -> Cut {
        let now: Vec<&Segment> = self.segments.iter().filter(|s| s.disc == self.disc).collect();
        let (Some(first), Some(newest)) = (now.first(), now.last()) else { return Cut::Wait };
        let near = |t: u64| t.abs_diff(at_ns) <= tol_ns;
        if at_ns + tol_ns < first.start_ns {
            return Cut::Drop;
        }
        if let Some(s) = now.iter().find(|s| near(s.start_ns)) {
            return Cut::Starts { msn: s.msn, disc: s.disc, pdt_ms: s.pdt_ms };
        }
        let end = newest.start_ns + newest.duration_ns();
        if newest.complete && near(end) {
            let pdt_ms = newest.pdt_ms + (newest.duration_ns() / 1_000_000) as i64;
            return Cut::Starts { msn: newest.msn + 1, disc: newest.disc, pdt_ms };
        }
        let Some(s) = now.iter().rev().find(|s| s.start_ns < at_ns) else { return Cut::Drop };
        match (s.msn == newest.msn, s.complete) {
            (false, _) => Cut::Inside,
            (true, true) if at_ns < end => Cut::Inside,
            (true, false) if at_ns + tol_ns < s.start_ns + min_ns => Cut::Inside,
            _ => Cut::Wait,
        }
    }

    /// The discontinuity and wall clock of segment `msn`, or of the one
    /// after the newest, which starts where the newest ends.
    pub fn meta(&self, msn: u64) -> Option<(u32, i64)> {
        if let Some(s) = self.segments.iter().find(|s| s.msn == msn) {
            return Some((s.disc, s.pdt_ms));
        }
        let newest = self.segments.back().filter(|s| s.complete && s.msn + 1 == msn)?;
        Some((newest.disc, newest.pdt_ms + (newest.duration_ns() / 1_000_000) as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hls::ring::Part;
    use bytes::Bytes;

    const MS: u64 = 1_000_000;

    fn segment(r: &mut Ring, start: u64, len: u64, whole: bool) {
        r.begin(start * MS, 2000 * MS, 1_000_000 + start as i64);
        r.push_part(Part { bytes: Bytes::from_static(b"v"), duration_ns: len * MS, independent: true });
        if whole {
            r.close();
        }
    }

    #[test]
    fn a_keyframe_is_a_start_inside_or_not_known_yet() {
        let mut r = Ring::new(10);
        r.set_init(Bytes::from_static(b"i"));
        assert_eq!(r.cut_at(0, 2000 * MS, 5 * MS), Cut::Wait, "nothing cut yet");
        segment(&mut r, 0, 2000, true);
        assert_eq!(r.cut_at(0, 2000 * MS, 5 * MS), Cut::Starts { msn: 0, disc: 0, pdt_ms: 1_000_000 });
        assert_eq!(r.cut_at(1000 * MS, 2000 * MS, 5 * MS), Cut::Inside);
        assert_eq!(r.cut_at(2001 * MS, 2000 * MS, 5 * MS), Cut::Starts { msn: 1, disc: 0, pdt_ms: 1_002_000 }, "the end of the newest");
        assert_eq!(r.cut_at(3000 * MS, 2000 * MS, 5 * MS), Cut::Wait, "past it: wait for the ring");
        segment(&mut r, 2000, 500, false);
        assert_eq!(r.cut_at(2300 * MS, 2000 * MS, 5 * MS), Cut::Inside, "an open segment is at least the shortest");
        assert_eq!(r.cut_at(4100 * MS, 2000 * MS, 5 * MS), Cut::Wait);
        assert_eq!(r.meta(1), Some((0, 1_002_000)));
        assert_eq!(r.meta(2), None, "the newest is open");
    }
}
