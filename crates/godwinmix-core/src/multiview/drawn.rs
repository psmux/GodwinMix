//! Whether the mosaic has published a frame drawn with every tile in it.
//!
//! `MultiviewHandle::warm` says every tile has delivered a buffer to its
//! compositor pad. That is not the same as a frame with the tile in it having
//! come out of the far end. The compositor had drawn black frames before the
//! tile arrived, and up to four of those are still between it and the
//! appsink (the two frame queue, the converter and the encoder, the appsink's
//! one). A still served the moment `warm` turned true was one of them, black
//! where the colour bars should have been, as the macOS runner showed.
//!
//! So the compositor's output is watched as well. The aggregator only starts
//! a frame once it has pushed the one before, so the second frame it pushes
//! with every tile fed was composited after every tile's first buffer had
//! reached its pad. The third is the one counted, which leaves a frame for
//! the moment between the pad's probe and the buffer being queued on it.
//! When a frame with that one's timestamp or a later one reaches the
//! appsink, the mosaic has published a drawn frame. A tile added later makes
//! it not warm again, and the count starts over.

use gstreamer as gst;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

const NONE: u64 = u64::MAX;

/// Frames pushed with every tile fed before one counts as drawn.
const SETTLE: u32 = 2;

#[derive(Debug)]
pub struct Drawn {
    /// Frames the compositor has pushed since every tile was fed.
    since_fed: AtomicU32,
    /// Timestamp of the first frame that counts, in nanoseconds, or NONE.
    from: AtomicU64,
    /// A frame at or after `from` has been published.
    published: AtomicBool,
}

impl Default for Drawn {
    fn default() -> Self {
        Drawn { since_fed: AtomicU32::new(0), from: AtomicU64::new(NONE), published: AtomicBool::new(false) }
    }
}

impl Drawn {
    /// The compositor pushed a frame at `pts`, with every tile fed or not.
    /// On its streaming thread: atomics only.
    pub fn composited(&self, all_fed: bool, pts: Option<gst::ClockTime>) {
        if !all_fed {
            self.reset();
            return;
        }
        if self.since_fed.fetch_add(1, Ordering::AcqRel) == SETTLE {
            self.from.store(pts.map_or(0, |t| t.nseconds()), Ordering::Release);
        }
    }

    /// A frame at `pts` has gone out to the subscribers.
    pub fn published_at(&self, pts: Option<gst::ClockTime>) {
        let from = self.from.load(Ordering::Acquire);
        if from != NONE && pts.is_none_or(|t| t.nseconds() >= from) {
            self.published.store(true, Ordering::Release);
        }
    }

    /// Whether a frame drawn with every tile in it has been published.
    pub fn published(&self) -> bool {
        self.published.load(Ordering::Acquire)
    }

    /// A new pipeline, or a tile that has not delivered yet.
    pub fn reset(&self) {
        self.since_fed.store(0, Ordering::Release);
        self.from.store(NONE, Ordering::Release);
        self.published.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64) -> Option<gst::ClockTime> {
        Some(gst::ClockTime::from_mseconds(ms))
    }

    #[test]
    fn the_frames_already_on_their_way_when_the_tiles_arrive_do_not_count() {
        let d = Drawn::default();
        d.composited(false, at(0));
        d.composited(true, at(100));
        d.composited(true, at(200));
        // Frames composited before every tile was waiting reach the appsink.
        d.published_at(at(0));
        d.published_at(at(100));
        d.published_at(at(200));
        assert!(!d.published(), "a frame composited too early counted as drawn");
        d.composited(true, at(300));
        d.published_at(at(200));
        assert!(!d.published());
        d.published_at(at(300));
        assert!(d.published());
    }

    #[test]
    fn a_tile_that_has_not_delivered_starts_it_over() {
        let d = Drawn::default();
        for ms in [0, 100, 200] {
            d.composited(true, at(ms));
        }
        d.published_at(at(200));
        assert!(d.published());
        d.composited(false, at(300));
        assert!(!d.published(), "a new black tile is not drawn");
        d.published_at(at(300));
        assert!(!d.published());
    }
}
