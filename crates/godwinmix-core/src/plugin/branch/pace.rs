//! A clip's frames, let into the programme shortly before they are due.
//!
//! A clip is not live. Left alone it decodes as fast as the mixer takes its
//! frames, and the compositor takes a frame onto a pad as long as the pad
//! holds less than its latency, which is a second (`MIN_UPSTREAM_LATENCY_NS`
//! in mixer.rs). So every clip kept a second of canvas frames on its
//! compositor pad, 40 MB at 720p, where a live camera keeps one or two. A
//! `clocksync` ahead of the clip's programme queue holds each frame until
//! `LEAD` before its time on the programme, and the decoder waits behind it.
//!
//! The compositor's timing does not change: a frame is still drawn at the
//! same output time, it has only waited in the decoder rather than on the
//! pad. `LEAD` is below the 200 ms at which the catch up guard
//! (`mixer::catch_up`) would judge the source to be running ahead.

use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::debug;

/// How long before its time a clip frame reaches the compositor.
pub const LEAD: i64 = 150_000_000;

/// A `clocksync` for source `id`, or `None` where this GStreamer has none,
/// in which case the clip runs ahead as it always did.
pub fn make(id: &str) -> Option<gst::Element> {
    let el = crate::gstutil::make("clocksync", &format!("pgm-vpace-{id}")).ok();
    if el.is_none() {
        debug!(source = %id, "no clocksync here; the clip is not paced");
    }
    el
}

/// Keep the pace on the source's timeline: `offset` is what its compositor
/// pads add to its running time, and the frame is let go `LEAD` before that.
pub fn follow(el: &gst::Element, offset: i64) {
    if el.find_property("ts-offset").is_some() {
        el.set_property("ts-offset", offset - LEAD);
    }
}
