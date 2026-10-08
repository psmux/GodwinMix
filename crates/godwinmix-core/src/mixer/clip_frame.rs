//! Where in a clip its newest picture sits, for showing that picture again.
//!
//! A held clip is shown again by seeking to its last frame
//! (`Mixer::show_held_clips_again`). The container's duration is not that
//! place: it is the longest of the clip's streams, and a phone's clip carries
//! sound a frame or more past its last picture, so a seek to the end of the
//! duration landed after every picture and sent none, and the tile stayed
//! black. The last picture that went past is the place, so it is kept: one
//! atomic store per frame, on the branch's video queue.

use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const NONE: u64 = u64::MAX;

pub struct LastFrame {
    /// Stream time of the newest frame, in nanoseconds, or `NONE`.
    at: AtomicU64,
}

impl Default for LastFrame {
    fn default() -> Self {
        Self { at: AtomicU64::new(NONE) }
    }
}

impl LastFrame {
    /// Keep the place of every frame that leaves through `pad`.
    pub fn watch(pad: &gst::Pad) -> Arc<Self> {
        let this = Arc::new(Self::default());
        let seen = this.clone();
        pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
            if let Some(gst::PadProbeData::Buffer(buffer)) = &info.data {
                if let Some(at) = stream_time(pad, buffer.pts()) {
                    seen.at.store(at.nseconds(), Ordering::Relaxed);
                }
            }
            gst::PadProbeReturn::Ok
        });
        this
    }

    /// Where the newest frame sits in the clip, once one has gone past.
    pub fn at(&self) -> Option<gst::ClockTime> {
        let at = self.at.load(Ordering::Relaxed);
        (at != NONE).then(|| gst::ClockTime::from_nseconds(at))
    }
}

fn stream_time(pad: &gst::Pad, pts: Option<gst::ClockTime>) -> Option<gst::ClockTime> {
    let event = pad.sticky_event::<gst::event::Segment>(0)?;
    let segment = event.segment().downcast_ref::<gst::ClockTime>()?;
    segment.to_stream_time(pts?)
}
