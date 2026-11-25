//! A clip's seek, which a clip with no sound must not refuse.
//!
//! A pipeline sends a seek to every sink it holds and answers yes only if all
//! of them took it. A clip with no audio track still has the audio half of the
//! normaliser and its proxy, with nothing upstream of them to take a seek, so
//! that sink said no while the picture's sink had moved the clip. The mixer
//! read the no as a failure and rebuilt the source, on every loop of a clip set
//! to `repeat`. On 0.3.25 a video only clip without faststart went round that
//! way, and so does one with it: the container was never the cause.
//!
//! The rebuild cost more than the seek. The new pipeline's timeline starts
//! again from the beginning, so the slot queue on the programme side read its
//! level as 0 ms behind the frames already through it and filled to its byte
//! backstop, 186 MB of 1080p, once a loop (`slot-q-5`, five times an hour).
//!
//! So when the pipeline says no, the same seek is sent through the picture's
//! own sink. The demuxer behind it flushes every stream it has, sound
//! included, so a clip that does have sound loses nothing by it; and the sinks
//! still reset the pipeline's time on the flush, as for any flushing seek.

use anyhow::{bail, Result};
use gstreamer as gst;
use gstreamer::prelude::*;

/// Seek `pipeline` to `position`, falling back to its picture's sink `video`
/// when a sink with nothing behind it refuses.
pub(crate) fn clip(pipeline: &gst::Pipeline, video: &gst::Element, flags: gst::SeekFlags, position: gst::ClockTime) -> Result<()> {
    if pipeline.seek_simple(flags, position).is_ok() {
        return Ok(());
    }
    let event = gst::event::Seek::new(1.0, flags, gst::SeekType::Set, position, gst::SeekType::None, gst::ClockTime::NONE);
    if video.send_event(event) {
        return Ok(());
    }
    bail!("neither the pipeline nor its picture took the seek")
}

#[cfg(test)]
#[path = "seek_tests.rs"]
mod tests;
