//! A slot never hands the compositor a picture with no segment in front of it.
//!
//! `compositor` places every frame by its running time, and a frame it cannot
//! place is a `g_assert` in `gst_video_aggregator_fill_queues` that aborts the
//! process. The slot pool flushes a slot's chain when it rebinds it, and a
//! flush takes the segment off every pad it passes. The source's segment is
//! sent again by the valve at the head of the slot, but a valve only sends what
//! it thinks the pad below has not seen, so a source coming back to the slot
//! it had before could send a frame into a chain that had lost its segment.
//! That aborted the installed app on 2026-10-05 with two phones on air.
//! `Pool::unbind` now flushes from the valve's own pad so the valve knows; this
//! is the second line, for any path into a slot that forgets.
//!
//! On the slot queue's sink pad, for each frame: a segment in time is there,
//! and nothing happens. It is not: the segment the valve last received is
//! sent down first, a copy so that no pad mistakes it for one it has passed
//! on already, and the frame follows it. With no segment to send at all, the
//! frame is dropped.

use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use tracing::warn;

/// Install the guard on slot `index`, whose valve is `gate` and whose queue
/// is `queue`.
pub(crate) fn guard_slot(index: usize, gate: &gst::Element, queue: &gst::Element) -> Result<()> {
    let entry = queue.static_pad("sink").context("a slot's queue has no sink pad")?;
    let above = gate.static_pad("sink").context("a slot's valve has no sink pad")?;
    entry
        .add_probe(gst::PadProbeType::BUFFER | gst::PadProbeType::BUFFER_LIST, move |pad, _| {
            if timed(pad).is_some() {
                return gst::PadProbeReturn::Ok;
            }
            let Some(segment) = timed(&above) else {
                warn!(slot = index, "a frame reached a slot with no segment to place it by; dropped");
                return gst::PadProbeReturn::Drop;
            };
            warn!(slot = index, "a frame reached a slot that had lost its segment; sent it again");
            pad.send_event(gst::event::Segment::new(&segment));
            gst::PadProbeReturn::Ok
        })
        .context("installing a slot's segment guard")?;
    Ok(())
}

/// The time segment stored on `pad`, if it has one.
fn timed(pad: &gst::Pad) -> Option<gst::FormattedSegment<gst::ClockTime>> {
    let event = pad.sticky_event::<gst::event::Segment>(0)?;
    event.segment().downcast_ref::<gst::ClockTime>().cloned()
}
