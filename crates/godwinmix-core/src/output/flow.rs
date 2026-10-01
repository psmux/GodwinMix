//! Whether bytes are actually leaving an output, counted at its sink.
//!
//! An output used to be `live` when its kind said it was connected, and that
//! answer outlived the data: a sidecar output whose plugin had stopped
//! reading still had a running process and said `live` for the rest of the
//! scale harness's run while nothing went out. The restreamer already counts
//! the bytes it sends, and this does the same for an output: a probe on the
//! sink pads of every sink in the output's pipeline adds up what reaches
//! them, and the output is only live while that number keeps moving.

use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How long an output may go without a byte reaching its sink before it stops
/// counting as live. The programme never stops, so a working output moves
/// bytes many times a second; this only has to be longer than a hiccup.
pub const STILL_FOR: Duration = Duration::from_secs(3);

#[derive(Default)]
pub struct Flow {
    /// A counter per generation, so a retired pipeline still finishing its
    /// last write cannot count towards the one that replaced it.
    bytes: Mutex<Arc<AtomicU64>>,
    /// What the pipelines before this one sent, for the running total.
    earlier: AtomicU64,
    /// Whether this generation's pipeline had a sink to count at. A kind
    /// whose sink cannot be found is judged on what it says alone.
    counted: AtomicBool,
    /// The count when it was last seen to move, and when that was.
    seen: Mutex<Option<(u64, Instant)>>,
}

impl Flow {
    /// Start counting a freshly built pipeline from nothing.
    pub fn watch(&self, pipeline: &gst::Pipeline) {
        let fresh = Arc::new(AtomicU64::new(0));
        let old = std::mem::replace(&mut *self.bytes.lock(), fresh.clone());
        self.earlier.fetch_add(old.load(Ordering::Relaxed), Ordering::Relaxed);
        *self.seen.lock() = None;
        let mut counted = false;
        for sink in pipeline.iterate_sinks().into_iter().flatten() {
            for pad in sink.sink_pads() {
                let bytes = fresh.clone();
                pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |_, info| {
                    let n = match &info.data {
                        Some(gst::PadProbeData::Buffer(b)) => b.size(),
                        Some(gst::PadProbeData::BufferList(l)) => l.calculate_size(),
                        _ => 0,
                    };
                    bytes.fetch_add(n as u64, Ordering::Relaxed);
                    gst::PadProbeReturn::Ok
                });
                counted = true;
            }
        }
        self.counted.store(counted, Ordering::Relaxed);
    }

    /// Everything that has reached the sink since the pipeline was built.
    pub fn bytes(&self) -> u64 {
        self.bytes.lock().load(Ordering::Relaxed)
    }

    /// Everything every pipeline of this output has sent, for `bytes_out`.
    pub fn total(&self) -> u64 {
        self.earlier.load(Ordering::Relaxed) + self.bytes()
    }

    /// True while bytes are reaching the sink: some have, and the count moved
    /// within `STILL_FOR` of `now`.
    pub fn moving(&self, now: Instant) -> bool {
        if !self.counted.load(Ordering::Relaxed) {
            return true;
        }
        let bytes = self.bytes();
        if bytes == 0 {
            return false;
        }
        let mut seen = self.seen.lock();
        match *seen {
            Some((before, at)) if before == bytes => now.duration_since(at) < STILL_FOR,
            _ => {
                *seen = Some((bytes, now));
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_reaching_the_sink_are_counted_and_a_stop_is_noticed() {
        let _ = gst::init();
        let p = gst::parse::launch("videotestsrc num-buffers=5 ! video/x-raw,width=64,height=64 ! fakesink")
            .unwrap()
            .downcast::<gst::Pipeline>()
            .unwrap();
        let flow = Flow::default();
        flow.watch(&p);
        let t0 = Instant::now();
        assert!(!flow.moving(t0), "nothing has been sent yet");
        p.set_state(gst::State::Playing).unwrap();
        let bus = p.bus().unwrap();
        bus.timed_pop_filtered(gst::ClockTime::from_seconds(5), &[gst::MessageType::Eos]).expect("the five frames ran");
        let _ = p.set_state(gst::State::Null);
        // Five 64x64 I420 frames.
        assert_eq!(flow.bytes(), 5 * 64 * 64 * 3 / 2);
        assert!(flow.moving(t0));
        assert!(flow.moving(t0 + Duration::from_secs(2)));
        assert!(!flow.moving(t0 + STILL_FOR + Duration::from_secs(1)), "a count that stopped still read as moving");
        // A new pipeline counts from nothing and the total carries on.
        flow.watch(&p);
        assert_eq!(flow.bytes(), 0);
        assert_eq!(flow.total(), 5 * 64 * 64 * 3 / 2);
    }
}
