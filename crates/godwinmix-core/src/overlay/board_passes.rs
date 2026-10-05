//! The board's probe, put on and taken off as things to draw come and go,
//! and the passes painted over the whole frame.

use super::Board;
use gstreamer::prelude::*;
use std::sync::Arc;

impl Board {
    /// Put the drawing probe on, if it is not on already.
    pub(in crate::overlay) fn ensure_probe(self: &Arc<Self>) {
        let mut probe = self.probe.lock();
        if probe.is_none() {
            *probe = crate::overlay::draw::install(self);
        }
    }

    /// Take the probe off when nothing is left to draw.
    /// The probe's lock is held across the check, so an add that lands
    /// between the two waits for it and puts the probe back on.
    pub(in crate::overlay) fn drop_probe_if_idle(&self) {
        let mut probe = self.probe.lock();
        if !self.entries.lock().is_empty() || !self.passes.lock().is_empty() {
            return;
        }
        if let (Some(id), Some(pad)) = (probe.take(), self.compositor.static_pad("src")) {
            pad.remove_probe(id);
        }
    }

    /// Paint pass over every programme frame until it finishes or is
    /// taken off. Answers the number emove_pass takes.
    pub fn add_pass(self: &Arc<Self>, pass: Arc<dyn crate::overlay::pass::Pass>) -> u64 {
        let id = self.passes.lock().add(pass);
        self.ensure_probe();
        id
    }

    /// Take a pass off, and the probe with it when nothing else is drawn.
    pub fn remove_pass(&self, id: u64) {
        self.passes.lock().remove(id);
        self.drop_probe_if_idle();
    }

    /// The passes for this frame, finished ones dropped.
    pub(in crate::overlay) fn live_passes(&self) -> Vec<(u64, Arc<dyn crate::overlay::pass::Pass>)> {
        self.passes.lock().live()
    }

    /// What each pass cost this frame. A pass slower than a frame for
    /// `SLOW_FRAMES` frames running is taken off, and said so once.
    pub(in crate::overlay) fn passes_spent(&self, costs: &[(u64, std::time::Duration)], budget: std::time::Duration) {
        for name in self.passes.lock().spent(costs, budget) {
            tracing::warn!(fx = %name, budget_ms = budget.as_millis() as u64, "an effect took longer than a frame to draw, frame after frame, so it was taken off to keep the programme on time. Use a smaller canvas, a release build, or another effect on this machine");
        }
    }
}
