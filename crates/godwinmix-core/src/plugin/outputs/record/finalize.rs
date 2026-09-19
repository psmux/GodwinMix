//! Finalisation happens off the mixer thread; normal exit waits a bounded time.
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

static PENDING: AtomicUsize = AtomicUsize::new(0);
struct Pending;
impl Drop for Pending {
    fn drop(&mut self) {
        PENDING.fetch_sub(1, Ordering::Release);
    }
}

pub fn finish(pipeline: gst::Pipeline) {
    let fallback = pipeline.clone();
    PENDING.fetch_add(1, Ordering::Release);
    let result = std::thread::Builder::new()
        .name("record-finalize".into())
        .spawn(move || {
            let _pending = Pending;
            // Disconnect the proxy producers before ending only the recorder's queues.
            for element in pipeline.children() {
                if element.factory().is_some_and(|f| f.name() == "proxysrc") {
                    if let Some(pad) = element.static_pad("src") {
                        if let Some(peer) = pad.peer() {
                            let _ = pad.unlink(&peer);
                            peer.send_event(gst::event::Eos::new());
                        }
                    }
                }
            }
            if let Some(bus) = pipeline.bus() {
                if let Some(message) = bus.timed_pop_filtered(
                    gst::ClockTime::from_seconds(5),
                    &[gst::MessageType::Eos, gst::MessageType::Error],
                ) {
                    if let gst::MessageView::Error(error) = message.view() {
                        tracing::warn!(error = %error.error(), "recording finalisation failed");
                    }
                } else {
                    tracing::warn!("recording finalisation timed out; inspect the last file");
                }
            }
            let _ = pipeline.set_state(gst::State::Null);
        });
    if result.is_err() {
        PENDING.fetch_sub(1, Ordering::Release);
        let _ = fallback.set_state(gst::State::Null);
    }
}

/// Called only during engine shutdown, never while processing a live command.
pub fn wait() {
    let deadline = Instant::now() + Duration::from_secs(6);
    while PENDING.load(Ordering::Acquire) != 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    if PENDING.load(Ordering::Acquire) != 0 {
        tracing::warn!(
            "recordings are still finishing after shutdown deadline; inspect the last files"
        );
    }
}
