//! One `textoverlay` per render thread, made once and used for every render.
//!
//! Making a `textoverlay` is where the time went. Pango builds a context and
//! matches the font every time one is made, and on Windows that measured 330
//! to 690 ms an element, growing as more were made, against 10 to 80 ms for
//! the whole run of the pipeline it was made for. A text source made a new one
//! for every change of its words, twice when the board drew it at another
//! size, so a `source.set` took about three quarters of a second to reach the
//! programme on a quiet machine and seconds on a busy one. Each text source
//! renders on a thread of its own, so a thread local is one element per
//! source, made at its first render and let go with its thread.

use anyhow::Result;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::cell::RefCell;

thread_local! {
    static KEPT: RefCell<Option<gst::Element>> = const { RefCell::new(None) };
}

/// This thread's `textoverlay`, made now if it has none. Taken out, so a
/// render that fails half way leaves nothing half used behind.
pub fn take() -> Result<gst::Element> {
    match KEPT.with(|kept| kept.borrow_mut().take()) {
        Some(overlay) => Ok(overlay),
        None => crate::gstutil::make("textoverlay", "text-render"),
    }
}

/// Put it back once `pipeline` is down, out of the pipeline so the next one
/// can hold it. Every property the next render needs is set again then.
pub fn give_back(pipeline: &gst::Pipeline, overlay: gst::Element) {
    if pipeline.remove(&overlay).is_ok() && overlay.current_state() == gst::State::Null {
        KEPT.with(|kept| *kept.borrow_mut() = Some(overlay));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_element_comes_back_after_a_pipeline_lets_it_go() {
        gst::init().unwrap();
        let first = take().unwrap();
        let pipeline = gst::Pipeline::new();
        pipeline.add(&first).unwrap();
        let _ = pipeline.set_state(gst::State::Null);
        give_back(&pipeline, first.clone());
        let again = take().unwrap();
        assert_eq!(again, first, "a render reuses this thread's element");
        assert!(again.parent().is_none(), "and it is free to join the next pipeline");
    }
}
