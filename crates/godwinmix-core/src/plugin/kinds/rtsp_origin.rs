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
//! So the stream is moved back by its first frame's running time, as a pad
//! offset on each of the decoder's output pads. One offset for picture and
//! sound, so lip sync is kept, decided by whichever frame arrives first.
//! Decided again for each new `rtspsrc`, which a restart makes.

use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;
use tracing::info;

#[derive(Default)]
struct State {
    /// How far the stream is moved back, once its first frame has said.
    offset: Option<i64>,
    /// The decoder's output pads this run, so a late one gets the same move.
    pads: Vec<glib::WeakRef<gst::Pad>>,
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
            return;
        }
    }
    let (state, id) = (state.clone(), id.to_string());
    // A blocking probe because the segment a new offset sends again goes out
    // after the blocking probes and before the rest, so this frame is moved
    // too. It never holds anything: `Pass` and `Remove` both let it through.
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BLOCK, move |pad, info| {
        let Some(buffer) = info.buffer() else { return gst::PadProbeReturn::Pass };
        let Some(running) = running_time(pad, buffer) else { return gst::PadProbeReturn::Pass };
        let mut s = state.lock();
        if s.offset.is_none() {
            s.offset = Some(running);
            for p in s.pads.iter().filter_map(|w| w.upgrade()) {
                p.set_offset(-running);
            }
            info!(source = %id, moved_ms = running / 1_000_000, "an RTSP stream's timeline was started at zero");
        }
        gst::PadProbeReturn::Remove
    });
}

/// Where `buffer` sits on its pad's running time, in nanoseconds.
fn running_time(pad: &gst::Pad, buffer: &gst::BufferRef) -> Option<i64> {
    let event = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = event.segment().downcast_ref::<gst::ClockTime>()?;
    let at = segment.to_running_time(buffer.pts().or(buffer.dts())?)?;
    Some(at.nseconds() as i64)
}
