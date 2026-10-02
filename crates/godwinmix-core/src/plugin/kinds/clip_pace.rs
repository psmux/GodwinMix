//! Playing a clip in real time from the moment its first frame arrives, at a
//! sink that is not the compositor.
//!
//! A source pipeline runs on the programme's clock with the programme's base
//! time, so its running time is however long the programme has been up, and a
//! clip's own timestamps start at zero. Synced as it is, every frame would be
//! late and shown at once, and the clip would decode as fast as the machine
//! allows. So on the first frame of each segment the sink's `ts-offset` is set
//! to the distance between the two, plus a little room, and the sink holds
//! each frame until its time comes.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// How far ahead of now the first frame is put, so it is never already late.
const ROOM: gst::ClockTime = gst::ClockTime::from_mseconds(40);

/// Install the probe on `sink`'s pad. Cheap: one atomic load per frame after
/// the first.
pub fn pace_from_now(sink: &gst_app::AppSink) {
    let Some(pad) = sink.static_pad("sink") else { return };
    let placed = Arc::new(AtomicBool::new(false));
    let element = sink.clone();
    pad.add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::EVENT_DOWNSTREAM, move |pad, info| {
        match &info.data {
            Some(gst::PadProbeData::Event(e)) if e.type_() == gst::EventType::Segment => {
                placed.store(false, Ordering::Release);
            }
            Some(gst::PadProbeData::Buffer(b)) if !placed.swap(true, Ordering::AcqRel) => {
                if let Some(offset) = offset(pad, &element, b.pts()) {
                    element.set_property("ts-offset", offset);
                }
            }
            _ => {}
        }
        gst::PadProbeReturn::Ok
    });
}

/// Nanoseconds to add to this buffer's running time to put it `ROOM` from now.
fn offset(pad: &gst::Pad, sink: &gst_app::AppSink, pts: Option<gst::ClockTime>) -> Option<i64> {
    let clock = sink.clock()?;
    let now = clock.time().checked_sub(sink.base_time()?)?;
    let segment = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = segment.segment().downcast_ref::<gst::ClockTime>()?.clone();
    let at = segment.to_running_time(pts?)?;
    Some((now + ROOM).nseconds() as i64 - at.nseconds() as i64)
}
