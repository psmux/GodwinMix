//! `livesync` run on the clock it is synced to, and the stream handed back
//! on its own timeline.
//!
//! A source pipeline shares the programme's clock and base time
//! (`Mixer::adopt_clock`), so its running time is the programme's: minutes,
//! on a show that has been up for minutes. Its stream does not start there.
//! An RTMP feed's first frame carries zero, and so does an RTSP one's once
//! `rtsp_origin` has moved it. The timeline aligner on the programme side
//! expects exactly that and adds the programme's running time to picture and
//! sound alike.
//!
//! `livesync` sits on the picture alone, with `sync` on, and its output
//! follows the clock. Handed a frame stamped zero with the clock a minute in,
//! it saw itself a minute behind: it repeated its last frame as fast as the
//! queues below would take, threw away the real frames as late, and came out
//! stamped on the clock's running time. The aligner then added the programme's
//! running time to that a second time, so the picture was placed as far in
//! the future as the programme was old, while the sound was placed right.
//!
//! At a first start the programme is a second old and the error was a second
//! of lip sync. After a cable pull on 2026-10-10 the restarted RTMP pull came
//! back 35 seconds into the show, read live for three seconds, filled its
//! queues with frames due half a minute later and read stalled again until
//! the next stall restart. An RTSP camera whose cable healed without a
//! restart had its fillers placed 3.2 s early and the catch up guard gave up
//! on it.
//!
//! So the stream is moved onto the clock on its way into `livesync`, by
//! however far its first frame is behind the clock, and moved back by the
//! same amount on its way out. `livesync` paces and fills against the clock as
//! it was written to; what leaves it is on the source's own timeline, beside
//! its sound, and the aligner places both once. Decided again for each new
//! stream, which a restart makes.

use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use tracing::info;

/// No move decided yet: the next frame decides it.
const UNDECIDED: i64 = i64::MIN;

/// Move the stream entering `sync`, a `livesync`, onto the clock at
/// `entry`, the src pad that feeds it, and back again at its own src pad.
pub fn wrap(entry: &gst::Pad, sync: &gst::Element, id: &str) {
    let Some(exit) = sync.static_pad("src") else { return };
    let shift = Arc::new(AtomicI64::new(UNDECIDED));
    // A new stream starts its stamps wherever its source starts them. Only a
    // new one: setting the offset sends every sticky event again, the stream
    // start among them, under the seqnum it already had.
    let (fresh, last) = (shift.clone(), Mutex::new(None));
    entry.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_, info| {
        let Some(event) = info.event().filter(|e| e.type_() == gst::EventType::StreamStart) else {
            return gst::PadProbeReturn::Ok;
        };
        if last.lock().replace(event.seqnum()) != Some(event.seqnum()) {
            fresh.store(UNDECIDED, Ordering::SeqCst);
        }
        gst::PadProbeReturn::Ok
    });
    let (sync, id) = (sync.downgrade(), id.to_string());
    // Blocking, like `rtsp_origin`: the segment a new offset sends again goes
    // out after the blocking probes and before the rest, so the frame that
    // decided the move is moved too. `Pass` never holds it.
    entry.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BLOCK, move |pad, info| {
        if shift.load(Ordering::SeqCst) != UNDECIDED {
            return gst::PadProbeReturn::Pass;
        }
        let Some(sync) = sync.upgrade() else { return gst::PadProbeReturn::Pass };
        let Some(by) = info.buffer().and_then(|b| behind_clock(pad, b, &sync)) else {
            return gst::PadProbeReturn::Pass;
        };
        shift.store(by, Ordering::SeqCst);
        pad.set_offset(by);
        exit.set_offset(-by);
        info!(source = %id, shift_ms = by / 1_000_000, "moved this source's picture onto the clock for livesync and back after it");
        gst::PadProbeReturn::Pass
    });
}

/// How far `buffer` is behind `sync`'s clock, on the pad's running time, in
/// nanoseconds; zero for one that is level with it or ahead. Never negative:
/// a negative pad offset past a segment's base lands in the segment's
/// `offset` field, and `livesync` 0.15 panics on that (see `rtsp_origin`).
fn behind_clock(pad: &gst::Pad, buffer: &gst::BufferRef, sync: &gst::Element) -> Option<i64> {
    let now = sync.clock()?.time().checked_sub(sync.base_time()?)?;
    let event = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = event.segment().downcast_ref::<gst::ClockTime>()?;
    let at = segment.to_running_time(buffer.pts().or(buffer.dts())?)?;
    Some(now.nseconds().saturating_sub(at.nseconds()) as i64)
}

#[cfg(test)]
#[path = "livesync_clock_tests.rs"]
mod tests;
