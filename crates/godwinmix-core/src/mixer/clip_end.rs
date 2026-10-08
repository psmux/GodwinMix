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
//! So a clip's end is judged where its programme branch hands frames to the
//! compositor: the EOS is caught on the far side of the branch's two queues,
//! after the last frame has gone out, and dropped there, so the compositor
//! pad never ends and goes on drawing that last frame. Then the mixer is told
//! (`Command::ClipEnded`) and does what `params.at_end` says; see
//! `mixer::clip_act`.
//!
//! The ad break is not one of these: its end is how the break knows to
//! return, and it keeps its own EOS handling.

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
    pub const NAMES: [&'static str; 3] = ["repeat", "hold", "leave"];

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
    /// Whether the mixer has been told about this pass's end. Cleared by the
    /// next segment, which is the clip playing again.
    told: AtomicBool,
    /// Holding its last frame: ended, and not asked to repeat.
    held: AtomicBool,
    /// When the start of the clip was, or would have been, on air. A repeat
    /// never begins sooner than one duration after it. See `wait_before_repeat`.
    began: Mutex<Instant>,
}

impl ClipEnd {
    fn new() -> ClipEnd {
        ClipEnd {
            started: AtomicU8::new(0),
            ended: AtomicU8::new(0),
            told: AtomicBool::new(false),
            held: AtomicBool::new(false),
            began: Mutex::new(Instant::now()),
        }
    }

    /// Install on the branch, before any data flows. `tell` runs on a
    /// streaming thread, once per end, and must not block.
    pub fn install(branch: &ProgrammeBranch, tell: impl Fn() + Send + Sync + 'static) -> Result<Arc<Self>> {
        let this = Arc::new(Self::new());
        let tell = Arc::new(tell);
        for (bit, queue) in [(VIDEO, &branch.vq), (AUDIO, &branch.aq)] {
            let pad = queue.static_pad("src").context("the branch queue has no src pad")?;
            let (watch, tell) = (this.clone(), tell.clone());
            pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
                let Some(event) = info.event() else { return gst::PadProbeReturn::Ok };
                watch.on_event(bit, event.type_(), &*tell)
            })
            .context("watching the branch for the end of the clip")?;
        }
        Ok(this)
    }

    fn on_event(&self, bit: u8, kind: gst::EventType, tell: &dyn Fn()) -> gst::PadProbeReturn {
        match kind {
            gst::EventType::Segment => {
                if self.told.swap(false, Ordering::AcqRel) {
                    self.started.store(0, Ordering::Release);
                    self.ended.store(0, Ordering::Release);
                    self.held.store(false, Ordering::Release);
                }
                self.started.fetch_or(bit, Ordering::AcqRel);
                gst::PadProbeReturn::Ok
            }
            gst::EventType::Eos => {
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

    pub fn hold(&self) {
        self.held.store(true, Ordering::Release);
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::Acquire)
    }

    /// Say where a seek landed, so the next repeat is timed from there. A
    /// seek plays a held clip again.
    pub fn landed_at(&self, position_ms: u64) {
        self.held.store(false, Ordering::Release);
        let back = Duration::from_millis(position_ms);
        let now = Instant::now();
        *self.began.lock() = now.checked_sub(back).unwrap_or(now);
    }

    /// How long to wait before repeating, if the end came sooner than the
    /// clip lasts. A clip with sound is paced by the audio mixer and a clip in
    /// a scene by the compositor; a clip with neither, on no scene, is pulled
    /// as fast as it decodes, and would go round as fast as that without this.
    pub fn wait_before_repeat(&self, duration_ms: Option<u64>) -> Option<Duration> {
        let due = *self.began.lock() + Duration::from_millis(duration_ms?);
        due.checked_duration_since(Instant::now()).filter(|d| !d.is_zero())
    }
}

#[cfg(test)]
#[path = "clip_end_tests.rs"]
mod tests;
