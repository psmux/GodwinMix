//! The guard's side of the timeline aligner: measuring each frame's lead as it
//! enters the programme pipeline, and acting on the supervisor's tick.

use super::{Verdict, Window};
use crate::mixer::TimelineAligner;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::{Arc, Weak};
use std::time::Instant;
use tracing::{info, warn};

impl TimelineAligner {
    /// Measure every buffer entering `queue`, the source's programme queue,
    /// into `pick`'s window. Two loads, a clock read and an atomic minimum.
    pub(in crate::mixer) fn watch_lead(
        self: &Arc<Self>,
        queue: &gst::Element,
        pick: fn(&TimelineAligner) -> &Window,
    ) {
        let Some(pad) = queue.static_pad("sink") else { return };
        let me: Weak<Self> = Arc::downgrade(self);
        pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
            let Some(me) = me.upgrade() else { return gst::PadProbeReturn::Remove };
            if let Some(lead) = info.buffer().and_then(|b| me.lead_of(pad, b)) {
                pick(&me).note(lead);
            }
            gst::PadProbeReturn::Ok
        });
    }

    /// How long `buffer` will wait on the programme's timeline before it is
    /// due, in nanoseconds. Negative when it arrived after its time.
    fn lead_of(&self, pad: &gst::Pad, buffer: &gst::BufferRef) -> Option<i64> {
        let offset = (*self.offset.lock())?;
        let now = self.clock.as_ref()?.time().checked_sub(self.base.unwrap_or(gst::ClockTime::ZERO))?;
        let event = pad.sticky_event::<gst::event::Segment>(0)?;
        let segment = event.segment().downcast_ref::<gst::ClockTime>()?;
        let running = segment.to_running_time(buffer.pts()?)?;
        Some(running.nseconds() as i64 + offset - self.catch.total() - now.nseconds() as i64)
    }

    /// One supervisor tick of the guard. A source that can be scrubbed is a
    /// file, which runs ahead of the clock by design, and is left alone.
    pub(in crate::mixer) fn keep_up(&self, seekable: bool, now: Instant) {
        if seekable {
            return;
        }
        let (verdict, least) = self.catch.review(now);
        match verdict {
            Verdict::Hold => {}
            Verdict::CatchUp(by) => {
                let total = self.catch.apply(by);
                if let Some(offset) = *self.offset.lock() {
                    self.tiles.set_offset(offset - total);
                }
                info!(
                    source = %self.id,
                    behind_ms = least.unwrap_or(0) / 1_000_000,
                    caught_up_ms = by / 1_000_000,
                    total_ms = total / 1_000_000,
                    "a live source was running behind its own frames; dropped to the newest"
                );
            }
            Verdict::GiveUp => warn!(
                source = %self.id,
                behind_ms = least.unwrap_or(0) / 1_000_000,
                "this source filled its queues again straight after a catch up, so it is not \
                 live; it is left as it is until it restarts"
            ),
        }
    }
}
