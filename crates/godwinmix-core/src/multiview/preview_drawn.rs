//! Whether the armed scene is in the preview's picture yet.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;

/// Whether the armed scene is in the preview's picture yet: every item has
/// had a frame reach its pad, and every filter drawn on the board (a key, a
/// cutout) has handed it a picture. A still of a filtered scene waits for
/// this; a freshly built preview composites its empty backdrop until then.
pub struct Drawn {
    pub(super) fed: Vec<Arc<std::sync::atomic::AtomicBool>>,
    pub(super) hooks: Vec<crate::plugin::filter::BoardHook>,
}

impl Drawn {
    pub fn now(&self) -> bool {
        self.fed.iter().all(|f| f.load(std::sync::atomic::Ordering::Acquire))
            && self.hooks.iter().all(|h| !h.layer.active() || h.layer.picture().is_some())
    }
}

/// A flag raised by the first buffer to reach `pad`. The probe goes with it.
pub(super) fn fed_by(pad: &gst::Pad) -> Arc<std::sync::atomic::AtomicBool> {
    let fed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = fed.clone();
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        flag.store(true, std::sync::atomic::Ordering::Release);
        gst::PadProbeReturn::Remove
    });
    fed
}
