//! An RTSP stream's timeline, started at zero the way every other stream's is.
//!
//! A source pipeline runs on the programme's clock and base time
//! (`Mixer::adopt_clock`). `rtspsrc` stamps each packet with when it arrived on
//! that clock, so an RTSP camera's first frame carries the programme's running
//! time: minutes, on a show that has been up for minutes. An RTMP camera's
//! first frame carries zero, and so does a clip's. The timeline aligner on the
//! programme side assumes zero: it shifts a source by the programme's running
//! time when its first segment arrives. An RTSP camera was shifted twice over,
//! every frame placed that far in the future, and the catch up guard pulled it
//! back two seconds later; but the frames already queued in the programme kept
//! their place, the compositor held them, the source's queues filled, nothing
//! more reached the programme and the source was judged stalled six seconds
//! after it went live. A restart did the same again.
//!
//! So the stream is moved back to start near zero, as a pad offset on each of
//! the decoder's output pads. One offset for picture and sound, so lip sync is
//! kept, decided again for each new `rtspsrc`, which a restart makes.
//!
//! The move is never more than a pad's segment can take out of its own base.
//! Past that, GStreamer puts the rest in the segment's `offset` field, and
//! `livesync` 0.15 then panics on the first frame (`imp.rs:1230`, an `unwrap`
//! of a running time it does not expect to be missing). So the move is the
//! least of each pad's first frame and each pad's segment start, on the
//! running time, lowered again if a pad that comes later starts earlier.

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::info;

#[derive(Default)]
struct State {
    /// How far the stream is moved back, once a first frame has said.
    offset: Option<i64>,
    /// The decoder's output pads this run, so every one gets the same move.
    pads: Vec<glib::WeakRef<gst::Pad>>,
}

impl State {
    /// Take `at` as the move if it is the first or the least so far.
    fn offer(&mut self, at: i64) -> bool {
        if self.offset.is_some_and(|o| o <= at) {
            return false;
        }
        self.offset = Some(at);
        for p in self.pads.iter().filter_map(|w| w.upgrade()) {
            p.set_offset(-at);
        }
        true
    }
}

/// Start `decode`'s output at zero. `decode` is a `uridecodebin` opening an
/// RTSP address; `id` names the source in the one log line this writes.
pub fn rebase(decode: &gst::Element, id: &str) {
    let state = Arc::new(Mutex::new(State::default()));
    let fresh = state.clone();
    // A new rtspsrc is a new run: its stamps start wherever the clock is now.
    decode.connect("source-setup", false, move |_| {
        *fresh.lock() = State::default();
        None
    });
    let id = id.to_string();
    decode.connect_pad_added(move |_, pad| watch(&state, pad, &id));
}

fn watch(state: &Arc<Mutex<State>>, pad: &gst::Pad, id: &str) {
    {
        let mut s = state.lock();
        s.pads.push(pad.downgrade());
        if let Some(offset) = s.offset {
            pad.set_offset(-offset);
        }
    }
    let (state, id) = (state.clone(), id.to_string());
    // A blocking probe because the segment a new offset sends again goes out
    // after the blocking probes and before the rest, so this frame is moved
    // too. It never holds anything: `Pass` and `Remove` both let it through.
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BLOCK, move |pad, info| {
        let Some(at) = info.buffer().and_then(|b| earliest(pad, b)) else { return gst::PadProbeReturn::Pass };
        if state.lock().offer(at) {
            info!(source = %id, moved_ms = at / 1_000_000, "an RTSP stream's timeline was started at zero");
        }
        gst::PadProbeReturn::Remove
    });
}

/// The earlier of `buffer` and its segment's start on the pad's running time,
/// in nanoseconds: as far as this pad can be moved back.
fn earliest(pad: &gst::Pad, buffer: &gst::BufferRef) -> Option<i64> {
    let event = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = event.segment().downcast_ref::<gst::ClockTime>()?;
    let frame = segment.to_running_time(buffer.pts().or(buffer.dts())?)?;
    let start = segment.start().and_then(|s| segment.to_running_time(s)).unwrap_or(frame);
    Some(frame.min(start).nseconds() as i64)
}
