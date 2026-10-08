//! Knowing when a clip has really ended, and what it was asked to do then.
//!
//! A clip used to end the way a dead camera does. Its pipeline posted EOS,
//! the supervisor took that as a source that had stopped, and restarted it in
//! place: the pipeline went to NULL, the source read `connecting`, and it came
//! back half a second later from the start. Two things were wrong with that
//! beyond the word on the screen. The EOS message is posted when the last
//! buffer leaves the source's own pipeline, and on this machine its programme
//! queues still held 933 ms of picture and a second of sound at that moment
//! (`vq_time_ms` and `aq_time_ms` in the timeline line); the restart flushed
//! them, so every pass lost the end of the clip. An eight second test clip
//! went round every 5.7 seconds.
//!
//! So the EOS is caught on the far side of the branch's two queues and dropped
//! there, so the compositor pad never ends and goes on drawing the last frame
//! it was given. Even there it is early: the compositor's own pad still holds
//! up to a second of frames waiting for their time. What does say when the
//! last frame is drawn is the clock. Each pass's first segment out of the
//! queue is the moment the aligner puts that segment's first frame on air, so
//! the last frame is due one duration, less where the segment started, after
//! it. The mixer is told at the EOS (`Command::ClipEnded`), waits out what is
//! left of that (`ClipEnd::wait_for_last_frame`), and then does what
//! `params.at_end` says; see `mixer::clip_act`.
//!
//! The ad break is not one of these: its end is how the break knows to
//! return, and it keeps its own EOS handling.

use super::clip_frame::LastFrame;
use crate::config::Params;
use crate::plugin::branch::ProgrammeBranch;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const VIDEO: u8 = 1;
const AUDIO: u8 = 2;

/// What a clip does when its last frame has gone out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtEnd {
    /// Play again from the start, by a seek, with no gap and no reconnect.
    Repeat,
    /// Stay on the last frame until it is scrubbed or restarted.
    Hold,
    /// Hold the last frame, and have the programme move off the clip.
    Leave,
}

impl AtEnd {
    /// From a clip's params. `hold` when nothing is said. A `loop = true`
    /// written by the OBS import before `at_end` existed still repeats.
    pub fn of(params: &Params) -> AtEnd {
        match params.get("at_end").and_then(|v| v.as_str()) {
            Some("repeat") => AtEnd::Repeat,
            Some("leave") => AtEnd::Leave,
            Some(_) => AtEnd::Hold,
            None if params.get("loop").and_then(|v| v.as_bool()) == Some(true) => AtEnd::Repeat,
            None => AtEnd::Hold,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            AtEnd::Repeat => "repeat",
            AtEnd::Hold => "hold",
            AtEnd::Leave => "leave",
        }
    }
}

/// Watches one clip's programme branch for the end of the clip.
pub struct ClipEnd {
    /// The branches that have carried a segment this pass.
    started: AtomicU8,
    /// The branches whose EOS has gone by this pass.
    ended: AtomicU8,
    /// Whether the mixer has been told about this pass's end.
    told: AtomicBool,
    /// A flush has come through: the next segment starts a new pass. A seek
    /// or a restart flushes both branches before either sends a segment, and
    /// the flush is the only way to tell the start of a pass from the other
    /// branch's segment arriving late in the same one. Near the end of a clip
    /// the picture can be through a whole pass before the sound's segment
    /// turns up.
    fresh: AtomicBool,
    /// Holding its last frame: ended, and not asked to repeat.
    held: AtomicBool,
    /// The next end is a held clip shown again, not a new end.
    quiet: AtomicBool,
    /// When the start of the clip was, or would have been, on air this pass.
    began: Mutex<Instant>,
    /// Where its newest picture sits in the clip. See `mixer::clip_frame`.
    last: Arc<LastFrame>,
}

impl ClipEnd {
    #[cfg(test)]
    fn new() -> ClipEnd {
        Self::with_last(Arc::default())
    }

    fn with_last(last: Arc<LastFrame>) -> ClipEnd {
        ClipEnd {
            last,
            started: AtomicU8::new(0),
            ended: AtomicU8::new(0),
            told: AtomicBool::new(false),
            fresh: AtomicBool::new(false),
            held: AtomicBool::new(false),
            quiet: AtomicBool::new(false),
            began: Mutex::new(Instant::now()),
        }
    }

    /// Install on the branch, before any data flows. `tell` runs on a
    /// streaming thread, once per end, and must not block.
    pub fn install(branch: &ProgrammeBranch, tell: impl Fn() + Send + Sync + 'static) -> Result<Arc<Self>> {
        let picture = branch.vq.static_pad("src").context("the branch queue has no src pad")?;
        let this = Arc::new(Self::with_last(LastFrame::watch(&picture)));
        let tell = Arc::new(tell);
        for (bit, queue) in [(VIDEO, &branch.vq), (AUDIO, &branch.aq)] {
            let pad = queue.static_pad("src").context("the branch queue has no src pad")?;
            let (watch, tell) = (this.clone(), tell.clone());
            pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
                let Some(event) = info.event() else { return gst::PadProbeReturn::Ok };
                watch.on_event(bit, event, &*tell)
            })
            .context("watching the branch for the end of the clip")?;
            // On the way in: the queue's src pad never sees a flush, which
            // `gstutil::stop_flushes_here` drops there.
            let sink = queue.static_pad("sink").context("the branch queue has no sink pad")?;
            let watch = this.clone();
            sink.add_probe(gst::PadProbeType::EVENT_FLUSH | gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
                if info.event().is_some_and(|e| e.type_() == gst::EventType::FlushStop) {
                    watch.fresh.store(true, Ordering::Release);
                }
                gst::PadProbeReturn::Ok
            })
            .context("watching the branch for a seek")?;
        }
        Ok(this)
    }

    fn on_event(&self, bit: u8, event: &gst::EventRef, tell: &dyn Fn()) -> gst::PadProbeReturn {
        match event.view() {
            gst::EventView::Segment(segment) => {
                if self.fresh.swap(false, Ordering::AcqRel) {
                    self.started.store(0, Ordering::Release);
                    self.ended.store(0, Ordering::Release);
                    self.told.store(false, Ordering::Release);
                    self.held.store(false, Ordering::Release);
                }
                if self.started.fetch_or(bit, Ordering::AcqRel) == 0 {
                    self.began_at(segment_start(segment.segment()));
                }
                gst::PadProbeReturn::Ok
            }
            gst::EventView::Eos(_) => {
                let ended = self.ended.fetch_or(bit, Ordering::AcqRel) | bit;
                let started = self.started.load(Ordering::Acquire);
                // Every branch that played this pass has finished. A clip with
                // no sound never starts the audio branch and is not waited on.
                if ended & started == started && !self.told.swap(true, Ordering::AcqRel) {
                    tell();
                }
                gst::PadProbeReturn::Drop
            }
            _ => gst::PadProbeReturn::Ok,
        }
    }

    /// The pass starts on air now, at `position` into the clip.
    fn began_at(&self, position: Duration) {
        let now = Instant::now();
        *self.began.lock() = now.checked_sub(position).unwrap_or(now);
    }

    /// Keep a clip's EOS off one more pad: a mosaic tile's entry, so the
    /// tile and the Studio preview drawn from it keep the last frame too.
    pub fn keep_last_frame(pad: &gst::Pad) {
        pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, |_pad, info| match info.event().map(|e| e.type_()) {
            Some(gst::EventType::Eos) => gst::PadProbeReturn::Drop,
            _ => gst::PadProbeReturn::Ok,
        });
    }

    /// The next end is a held clip showing its last frame again for a
    /// picture that was not there to get it the first time (see
    /// `Mixer::show_held_clips_again`), and is not news: it says nothing and
    /// leaves no scene.
    pub fn quietly(&self) {
        self.quiet.store(true, Ordering::Release);
    }

    /// Where the newest picture sits in the clip, once one has gone past.
    pub fn last_frame(&self) -> Option<gst::ClockTime> {
        self.last.at()
    }

    /// Held, or on its way back to held after showing its last frame again.
    /// A second picture made while the first is still being sent its frame
    /// asks again: what was already decoded went to the pictures that were
    /// there then.
    pub fn resting(&self) -> bool {
        self.held() || self.quiet.load(Ordering::Acquire)
    }

    pub fn take_quiet(&self) -> bool {
        self.quiet.swap(false, Ordering::AcqRel)
    }

    /// Whether this pass has ended. False again once the clip plays on, so a
    /// notice still waiting for the last frame of a pass that a scrub cut
    /// short finds nothing to do.
    pub fn at_end(&self) -> bool {
        self.told.load(Ordering::Acquire)
    }

    pub fn hold(&self) {
        self.held.store(true, Ordering::Release);
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }

    /// How long until the last frame of this pass is drawn, if it is not yet.
    /// Also what stops a clip that nothing paces (no sound, on no scene, so
    /// pulled as fast as it decodes) going round as fast as that.
    pub fn wait_for_last_frame(&self, duration_ms: Option<u64>) -> Option<Duration> {
        let due = *self.began.lock() + Duration::from_millis(duration_ms?);
        due.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())
    }
}

/// Where in the clip a segment starts: zero from the top, the position after
/// a seek.
fn segment_start(segment: &gst::Segment) -> Duration {
    segment
        .downcast_ref::<gst::ClockTime>()
        .and_then(|s| s.time().or(s.start()))
        .map_or(Duration::ZERO, |t| Duration::from_nanos(t.nseconds()))
}

#[cfg(test)]
#[path = "clip_end_tests.rs"]
mod tests;
