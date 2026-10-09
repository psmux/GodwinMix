//! Taking an output's feed out of the programme without holding the mixer.
//!
//! The feed is two leaky queues and two proxy sinks in the program pipeline.
//! Taking one to NULL joins its streaming thread, and that thread is wherever
//! the output's own pipeline holds it: inside `proxysink`, waiting for room
//! in a `proxysrc` queue that the recorder's muxer drains, or behind an
//! encoder's driver. On 2026-10-09 a tester pressed Stop recording with Quick
//! Sync encoding, and `output.remove` sat in that join on the mixer thread
//! for more than seventy seconds. The command queue filled to 256, every call
//! answered busy, and only killing the process brought it back.
//!
//! So a removal is split. `cut_off` takes the elements out of the program
//! bin, which unlinks them and waits for nothing: the tee's next push meets
//! an unlinked pad, which `allow-not-linked` ignores, and the id is free for
//! the next attach at once. `to_null` then takes each element down and
//! releases the tee pads, on the thread `Mixer::detach_off_thread` gives it,
//! which says in the log when it overruns. One that never finishes costs a
//! parked thread and four elements, not the mixer.

use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::warn;

/// Out of the program bin. Removing an element from a bin unlinks its pads,
/// and neither takes a stream lock, so this returns at once whatever the
/// streaming threads are doing. Locked first, so nothing that walks the
/// program's state can take them back up.
pub fn cut_off(program: &gst::Pipeline, elements: &[gst::Element]) {
    for el in elements {
        el.set_locked_state(true);
        if el.parent().is_some_and(|p| &p == program.upcast_ref::<gst::Object>()) {
            let _ = program.remove(el);
        }
    }
}

/// Each element to NULL, then the tee pads back to their tees. The tee pads
/// go last: one is unlinked already, and releasing it takes its stream lock,
/// which a push into an unlinked pad does not hold for long.
pub fn to_null(id: &str, elements: &[gst::Element], tee_pads: &[(gst::Element, gst::Pad)]) {
    for el in elements {
        if el.set_state(gst::State::Null).is_err() {
            warn!(output = %id, element = %el.name(), "an output's feed element would not go to NULL");
        }
    }
    for (tee, pad) in tee_pads {
        tee.release_request_pad(pad);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    /// A feed whose thread is parked inside the proxy sink, the way it is
    /// when the output's pipeline stops draining. The cut is immediate, and
    /// the element is no longer in the programme.
    #[test]
    fn a_feed_stuck_downstream_is_cut_off_without_waiting() {
        let _ = gst::init();
        let program = gst::parse::launch(
            "videotestsrc is-live=true ! tee name=t allow-not-linked=true ! queue name=feed leaky=downstream ! fakesink name=s async=false",
        )
        .unwrap()
        .downcast::<gst::Pipeline>()
        .unwrap();
        let (parked_tx, parked_rx) = mpsc::channel::<()>();
        let parked_tx = std::sync::Mutex::new(parked_tx);
        let sink = program.by_name("s").unwrap();
        sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let _ = parked_tx.lock().unwrap().send(());
            std::thread::sleep(Duration::from_secs(3600));
            gst::PadProbeReturn::Ok
        });
        program.set_state(gst::State::Playing).unwrap();
        parked_rx.recv_timeout(Duration::from_secs(5)).expect("a buffer reached the stuck sink");
        let feed = program.by_name("feed").unwrap();
        let started = Instant::now();
        cut_off(&program, &[feed.clone(), sink.clone()]);
        assert!(started.elapsed() < Duration::from_millis(500), "the cut waited {:?}", started.elapsed());
        assert!(program.by_name("feed").is_none(), "the feed is still in the programme");
        assert!(feed.parent().is_none());
        // The parked thread never returns; what it holds is left as it is.
        std::mem::forget((program, feed, sink));
    }
}
