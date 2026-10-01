//! Taking an output's old pipeline down without waiting on it for ever.
//!
//! Going to NULL waits for every streaming thread in the pipeline to come
//! back, and a sink's thread comes back when its write does. A write into a
//! FIFO whose reader has stopped reading, or into a socket the far end never
//! drains, does not come back at all. On 2026-10-01 the scale harness caught
//! exactly that: a udp/output plugin stopped reading its FIFO, the overflow
//! watchdog forced a reconnect, and the reconnect sat in `set_state(Null)` on
//! the old pipeline for the rest of the run. The output kept saying `live`,
//! because the reconnect held the lock its state is read through, and a
//! removal of the output then held the mixer's command loop as well.
//!
//! So NULL happens on a thread of its own and the caller waits a bounded time
//! for it. Past that the caller goes on and builds the next pipeline; the old
//! one finishes going down whenever its sink lets go, and a kind that knows
//! how to make it let go (a sidecar output stops the plugin process, which
//! closes the FIFO's read end and fails the write) does so.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::mpsc;
use std::time::Duration;
use tracing::warn;

/// How long a reconnect waits for the old pipeline. A pipeline whose sink is
/// working reaches NULL in milliseconds.
pub const RETIRE_WAIT: Duration = Duration::from_secs(2);

/// A pipeline still on its way to NULL, on its own thread.
pub struct Retiring {
    name: String,
    done: Option<mpsc::Receiver<()>>,
}

impl Retiring {
    /// Wait up to `within` more. True when it has reached NULL.
    pub fn wait(&self, within: Duration) -> bool {
        self.done.as_ref().is_some_and(|d| d.recv_timeout(within).is_ok())
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Take `pipeline` to NULL on a thread of its own and wait at most `within`.
///
/// `Ok` when it got there in time. When it did not, the thread carries on, the
/// pipeline goes when its sink lets go of it, and the `Retiring` says when.
pub fn to_null_within(pipeline: gst::Pipeline, within: Duration) -> Result<(), Retiring> {
    let name = pipeline.name().to_string();
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let spawned = std::thread::Builder::new().name(format!("retire-{name}")).spawn(move || {
        let _ = pipeline.set_state(gst::State::Null);
        let _ = done_tx.send(());
    });
    if let Err(e) = spawned {
        // The pipeline went with the closure. Nothing to wait for, and nothing
        // here may wait unbounded instead.
        warn!(pipeline = %name, ?e, "no thread to take an output pipeline down on; it is dropped as it is");
        return Err(Retiring { name, done: None });
    }
    if done_rx.recv_timeout(within).is_ok() {
        return Ok(());
    }
    warn!(
        pipeline = %name,
        waited_ms = within.as_millis() as u64,
        "the old output pipeline has not stopped: its sink is inside a write that has not returned. \
         Going on without it"
    );
    Err(Retiring { name, done: Some(done_rx) })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn a_pipeline_that_stops_is_reported_as_stopped() {
        let _ = gst::init();
        let p = gst::parse::launch("videotestsrc ! fakesink").unwrap().downcast::<gst::Pipeline>().unwrap();
        p.set_state(gst::State::Playing).unwrap();
        assert!(to_null_within(p.clone(), Duration::from_secs(5)).is_ok());
        assert_eq!(p.current_state(), gst::State::Null);
    }

    /// A sink stuck inside its render, the way a filesink sits in a write to
    /// a FIFO nobody reads. The wait gives up on time instead of joining it.
    #[test]
    fn a_sink_stuck_in_a_write_costs_the_bound_and_no_more() {
        let _ = gst::init();
        let p = gst::parse::launch("videotestsrc is-live=true ! fakesink name=s async=false")
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        let (parked_tx, parked_rx) = mpsc::channel::<()>();
        let parked_tx = std::sync::Mutex::new(parked_tx);
        let sink = p.by_name("s").unwrap();
        sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            let _ = parked_tx.lock().unwrap().send(());
            std::thread::sleep(Duration::from_secs(3600));
            gst::PadProbeReturn::Ok
        });
        p.set_state(gst::State::Playing).unwrap();
        parked_rx.recv_timeout(Duration::from_secs(5)).expect("a buffer reached the stuck sink");
        let started = Instant::now();
        let retiring = to_null_within(p.clone(), Duration::from_millis(300)).expect_err("it cannot stop");
        assert!(!retiring.wait(Duration::from_millis(100)));
        assert!(started.elapsed() < Duration::from_secs(2), "the wait was not bounded");
        // The parked thread never returns; the pipeline is left as it is.
        std::mem::forget(p);
    }
}
