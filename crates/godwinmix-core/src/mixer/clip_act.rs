//! What the mixer does when a clip's last frame has gone out, by its
//! `params.at_end`. See `mixer::clip_end` for how the end is found.
//!
//! * `repeat`: a flushing seek to the start, the same one a scrubber makes.
//!   The branch's queues are empty by then, so the flush throws nothing away;
//!   the flush stops at the queue, so what the compositor still holds plays
//!   out; and the seek is made when the last of that is due, so the first new
//!   frame lands one decode after the last old one, placed by the aligner at
//!   that moment. Nothing is restarted and the source stays `live`. A
//!   clip that cannot be seeked (a file over HTTP from a server that refuses
//!   ranges) is restarted as before, after its tail has played.
//! * `hold`, the default: nothing happens to the pipeline. The last frame
//!   stays up, the source reads `live` with `ended: true` in its status, and
//!   a seek or a restart plays it again.
//! * `leave`: held the same way, and `event/source.ended` says so. Moving the
//!   programme off the clip is a take, so the control plane makes it, through
//!   `program.take` like any other caller (`control::clip_leave`). Nothing on a
//!   streaming thread or this one waits for it.
//!
//! Every end is said in `event/source.ended`, with what the clip did.

use super::clip_end::{AtEnd, ClipEnd};
use super::{Command, Mixer, SeekOutcome};
use crate::input::InputPipeline;
use crate::plugin::branch::ProgrammeBranch;
use crate::state::{Event, SourceId};
use anyhow::Result;
use gstreamer as gst;
use std::sync::Arc;
use tracing::{debug, info, warn};

impl Mixer {
    /// The watcher for a new source, if it is a clip. Its notice carries the
    /// generation, so an end reported for a source since replaced is dropped.
    pub(super) fn watch_clip_end(
        &self,
        input: &InputPipeline,
        branch: &ProgrammeBranch,
        generation: u64,
    ) -> Result<Option<Arc<ClipEnd>>> {
        if !input.declares_seek() || input.id == super::AD_ID {
            return Ok(None);
        }
        let (handle, id) = (self.handle.clone(), input.id.clone());
        let tell = move || {
            let _ = handle.send(Command::ClipEnded(id.clone(), generation));
        };
        ClipEnd::install(branch, tell).map(Some)
    }

    /// A clip's last frame has gone out to the programme.
    pub(super) fn clip_ended(&mut self, id: &SourceId, generation: u64) {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id && s.generation == generation) else {
            debug!(source = %id, generation, "a clip ended that has since been removed or replaced");
            return;
        };
        let Some(end) = slot.clip_end.clone().filter(|e| e.at_end()) else { return };
        // The EOS left the branch with up to a second still queued at the
        // compositor; this is told again when the last of it is drawn.
        if let Some(wait) = end.wait_for_last_frame(slot.input.duration_ms()) {
            let (handle, again) = (self.handle.clone(), id.clone());
            self.rt.spawn(async move {
                tokio::time::sleep(wait).await;
                let _ = handle.send(Command::ClipEnded(again, generation));
            });
            return;
        }
        if !end.act_once() {
            debug!(source = %id, "a second notice of a clip's end that has been acted on");
            return;
        }
        let at_end = AtEnd::of(&slot.input.current_config().params);
        if end.take_quiet() && at_end != AtEnd::Repeat {
            end.hold();
            slot.input.mark_ended();
            self.broadcast_status();
            return;
        }
        if at_end != AtEnd::Repeat {
            end.hold();
            // Said here as well as on the bus's EOS, which a clip with no
            // sound never posts: its unlinked sound branch never ends.
            slot.input.mark_ended();
            info!(source = %id, at_end = at_end.as_str(), "the clip reached its end and holds its last frame");
            let _ = self.events.send(Event::SourceEnded { source: id.clone(), at_end: at_end.as_str().into() });
            self.broadcast_status();
            return;
        }
        if !slot.seekable() {
            self.arm_source_restart(id.clone(), "the clip reached its end and cannot be seeked back to the start");
            return;
        }
        let _ = self.events.send(Event::SourceEnded { source: id.clone(), at_end: at_end.as_str().into() });
        self.play_from_start(id);
    }

    /// Params taken in place. A clip held at its end that has just been set
    /// to repeat starts again now rather than waiting for a seek, and every
    /// page hears the new `at_end` at once: the Repeat toggle on the tile
    /// follows it, in this browser and in every other.
    pub(super) fn clip_reconfigured(&mut self, id: &SourceId) {
        let Some(slot) = self.sources.iter().find(|s| &s.input.id == id) else { return };
        let Some(end) = &slot.clip_end else { return };
        if end.held() && AtEnd::of(&slot.input.current_config().params) == AtEnd::Repeat {
            self.play_from_start(id);
        } else {
            self.broadcast_status();
        }
    }

    /// The source's own pipeline has read the clip to its end. What it sent
    /// is still playing out of the branch, so this is not a stall: nothing
    /// new arrives for the second or so that takes, and a stall judged then
    /// read `stalled` between every pass.
    pub(super) fn clip_read_to_end(&self, id: &str) -> bool {
        let Some(slot) = self.sources.iter().find(|s| s.input.id.as_str() == id) else { return false };
        if slot.clip_end.is_none() {
            return false;
        }
        slot.input.mark_ended();
        debug!(source = %id, "the clip has been read to its end; its branch says when it has played");
        true
    }

    /// What a clip does at its end, for its row in the status.
    pub(super) fn clip_status(slot: &super::SourceSlot, status: &mut crate::state::SourceStatus) {
        let Some(end) = &slot.clip_end else { return };
        status.put_extra("at_end", AtEnd::of(&slot.input.current_config().params).as_str());
        if end.held() {
            status.put_extra("ended", true);
        }
    }

    /// Show every held clip's last frame again, for pictures made since it
    /// came to rest.
    ///
    /// A held clip sends nothing more, so a mosaic built after it ended, a
    /// Studio preview of its scene armed after, or a compositor slot bound to
    /// it by a later take, had no frame of it and drew black. Called when one
    /// of those is made: a seek to the last frame decodes from the keyframe
    /// before it, once, and sends that frame down every branch. Its end is
    /// quiet (`ClipEnd::quietly`), so a clip set to leave the scene does not
    /// leave it again.
    pub(super) fn show_held_clips_again(&mut self) {
        for slot in &self.sources {
            let Some(end) = slot.clip_end.as_ref().filter(|e| e.resting()) else { continue };
            if !slot.seekable() {
                continue;
            }
            // The newest picture's own place: the duration can lie past every
            // picture, see `mixer::clip_frame`.
            let duration = slot.input.duration_ms().map(|ms| gst::ClockTime::from_mseconds(ms.saturating_sub(1)));
            let Some(at) = end.last_frame().or(duration) else { continue };
            end.quietly();
            // Placed again from now, as for any seek: see `Mixer::seek`.
            if let Some(aligner) = &slot.aligner {
                aligner.reset();
            }
            match slot.input.seek_exact(at) {
                Ok(()) => debug!(source = %slot.input.id, "showing a held clip's last frame again for a picture made since"),
                Err(e) => {
                    end.take_quiet();
                    warn!(source = %slot.input.id, ?e, "a held clip could not show its last frame again");
                }
            }
        }
    }

    fn play_from_start(&mut self, id: &SourceId) {
        match self.seek(id, 0) {
            SeekOutcome::Moved(_) => info!(source = %id, "the clip reached its end and plays again from the start"),
            other => {
                warn!(source = %id, outcome = ?other, "the clip could not be seeked back to its start; restarting it");
                self.arm_source_restart(id.clone(), "the clip reached its end and the seek back to the start failed");
            }
        }
    }
}
