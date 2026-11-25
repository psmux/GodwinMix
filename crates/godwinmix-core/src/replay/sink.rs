//! The appsink at the end of a replay branch, and a sample as a ring frame.

use super::branch::Side;
use super::ring::{Frame, Ring};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::Arc;

/// The appsink at the end: one sample, one push into the ring.
pub(super) fn sink(ring: &Arc<Mutex<Ring>>, side: Side, tag: &str) -> gst::Element {
    let sink = gst_app::AppSink::builder().name(format!("{tag}-sink")).sync(false).build();
    sink.set_property("async", false);
    sink.set_property("enable-last-sample", false);
    let ring = ring.clone();
    sink.set_callbacks(
        gst_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let base = sink.base_time().map(|t| t.nseconds()).unwrap_or(0);
                if let Some((frame, caps)) = frame_of(&sample, base) {
                    let mut ring = ring.lock();
                    match side {
                        Side::Video => ring.push_video(frame, &caps),
                        Side::Audio => ring.push_audio(frame, &caps),
                    }
                }
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    sink.upcast()
}

/// A sample as a ring frame, stamped in clock time: its running time plus the
/// pipeline's base time, which is the same clock in every pipeline here.
fn frame_of(sample: &gst::Sample, base: u64) -> Option<(Frame, gst::Caps)> {
    let buffer = sample.buffer_owned()?;
    let caps = sample.caps_owned()?;
    let segment = sample.segment().and_then(|s| s.downcast_ref::<gst::ClockTime>().cloned());
    let running = |t: Option<gst::ClockTime>| -> Option<u64> {
        let t = t?;
        let r = segment.as_ref().map_or(Some(t), |s| s.to_running_time(t))?;
        Some(r.nseconds() + base)
    };
    let at = running(buffer.pts())?;
    let dts = running(buffer.dts()).filter(|d| *d != at);
    let duration = buffer.duration().map(|d| d.nseconds()).unwrap_or(0);
    let key = !buffer.flags().contains(gst::BufferFlags::DELTA_UNIT);
    Some((Frame { at, dts, duration, key, buffer }, caps))
}
