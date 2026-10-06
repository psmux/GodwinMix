//! Binding a transition's curves so the compositor has not passed their start.

use super::transition::{Bound, Curve};
use super::Mixer;
use gstreamer as gst;
use tracing::debug;

/// How many times the start is moved on before the curves are left as they
/// are. Each try is a bind, which is property writes and nothing else.
const TRIES: usize = 4;

impl Mixer {
    /// Bind curves so their window starts on a frame the compositor has not
    /// made yet. Answers what was bound and where the window starts.
    ///
    /// `start` is the frame after the last one the compositor pushed, read
    /// before the curves were built. On a loaded machine this thread can be
    /// held off the processor between that read and the bind for longer than
    /// the whole window, and the compositor makes the window's frames with no
    /// curve bound: measured, the old scene stood still for all of a 300 ms
    /// wipe and the new one appeared 1166 ms in. So the start is checked
    /// once the curves are on: if the compositor has got there already, every
    /// curve moves on by the frames it missed and is bound again. Moving
    /// them is exact, since every curve is a function of the window's start.
    pub(super) fn bind_in_time(&mut self, mut curves: Vec<Curve>, mut start: gst::ClockTime) -> (Bound, gst::ClockTime) {
        let mut bound = self.controllers.bind(curves.clone());
        for _ in 0..TRIES {
            if self.pgm_out.running().is_none_or(|made| made < start) {
                break;
            }
            let later = self.compositor_now();
            let by = later.saturating_sub(start);
            debug!(late_ms = by.mseconds(), "the compositor passed the transition's start while it was bound; starting it later");
            for curve in &mut curves {
                for point in &mut curve.points {
                    point.0 += by;
                }
            }
            start = later;
            bound = self.controllers.bind(curves.clone());
        }
        (bound, start)
    }
}
