//! A file written from a running show starts its own clock at zero.
//!
//! The programme's buffers carry the programme pipeline's running time, and
//! that time passes through `proxysink`/`proxysrc` untouched. A muxer writes
//! running time, so a recording started two minutes into a show began at
//! 2:00: `mp4mux` put it in the first fragment's decode time and
//! `matroskamux` in its cluster timecodes, and a player read the file as two
//! minutes long. A muxer that streams to a network does not care; a file does.
//!
//! The first picture's running time becomes the file's zero. Both branches
//! are moved back by that same amount with a pad offset, which shifts the
//! segment rather than rewriting any buffer, so sound and picture keep the
//! distance they had. Sound from before the first picture is dropped: it
//! would land before zero, and a muxer cannot place it there.
//!
//! Everything here runs inside a pad probe on the recorder's own pipeline,
//! takes one short lock, and never waits. The programme encoder is upstream
//! of the proxy and never sees any of it.
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::Arc;

/// Rebase the two branches leaving `video` and `audio` so the file they feed
/// starts at zero. Call it once per build, before the pipeline plays.
pub fn start_at_zero(video: &gst::Element, audio: &gst::Element) -> Result<()> {
    let vpad = video.static_pad("src").context("the picture branch has no output")?;
    let apad = audio.static_pad("src").context("the sound branch has no output")?;
    let origin: Arc<Mutex<Option<gst::ClockTime>>> = Default::default();
    let first = origin.clone();
    vpad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(at) = earliest(pad, info) else {
            return gst::PadProbeReturn::Ok;
        };
        *first.lock() = Some(at);
        shift(pad, at);
        gst::PadProbeReturn::Remove
    });
    apad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(at) = earliest(pad, info) else {
            return gst::PadProbeReturn::Ok;
        };
        match *origin.lock() {
            Some(zero) if at >= zero => {
                shift(pad, zero);
                gst::PadProbeReturn::Remove
            }
            _ => gst::PadProbeReturn::Drop,
        }
    });
    Ok(())
}

/// The earlier of a buffer's decode and presentation times, as running time
/// in the segment its pad carries. None for a buffer with neither, or before
/// any segment, which the probe lets through and waits for the next.
fn earliest(pad: &gst::Pad, info: &gst::PadProbeInfo) -> Option<gst::ClockTime> {
    let Some(gst::PadProbeData::Buffer(buffer)) = &info.data else {
        return None;
    };
    let segment = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = segment.segment().downcast_ref::<gst::ClockTime>()?;
    let at = match (buffer.dts(), buffer.pts()) {
        (Some(dts), Some(pts)) => dts.min(pts),
        (dts, pts) => dts.or(pts)?,
    };
    segment.to_running_time(at)
}

/// Move everything leaving `pad` back by `by`. The pad sends its segment
/// again, with the offset applied, ahead of the buffer the probe is holding.
fn shift(pad: &gst::Pad, by: gst::ClockTime) {
    pad.set_offset(-(by.nseconds() as i64));
}
