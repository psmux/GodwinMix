//! Restarting and stopping sources without holding the mixer thread.
//!
//! The work itself happens on a thread from `offload`; this is the mixer's
//! half of it: what is marked before the work goes, what comes back when it
//! is done, and what has to wait for it. The order guarantees are the
//! pipeline's own (see `input::lifecycle`); the one kept here is that a source
//! is never added again under an id whose old pipeline is still stopping. A
//! page, a plugin process or a capture device can be claimed by one instance
//! at a time, so the add waits for the stop, for `STOP_GRACE` at most.

use super::{offload, Command, Mixer};
use crate::input::InputPipeline;
use crate::output::OutputSlot;
use crate::state::SourceId;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{debug, error, info, warn};

/// How long an add waits for the old pipeline under the same id to stop.
/// Well inside the five seconds a caller waits for an answer, so an add that
/// had to wait still answers the caller who asked for it.
pub const STOP_GRACE: Duration = Duration::from_secs(3);

/// A source whose pipeline is being stopped on a worker thread.
pub(super) struct Stopping {
    done: Arc<AtomicBool>,
    since: Instant,
    /// Adds of the same id that arrived meanwhile, sent again once it is done.
    parked: Vec<Command>,
}

impl Mixer {
    /// Restart a source that declares `restart-in-place`, on a thread of its
    /// own. The mixer carries on answering while it runs, and hears back with
    /// `SourceRestarted`.
    pub(super) fn restart_in_place(&mut self, id: &SourceId) {
        let Some(slot) = self.sources.iter_mut().find(|s| &s.input.id == id) else { return };
        if !slot.input.claim_restart() {
            debug!(source = %id, "a restart of this source is already running");
            return;
        }
        slot.stalled_ticks = 0;
        // A restarted source starts counting from zero again.
        if let Some(a) = &slot.aligner {
            a.reset();
        }
        let input = slot.input.clone();
        let generation = slot.generation;
        let handle = self.handle.clone();
        let back = id.clone();
        let started = offload::run("restart", id.as_str(), move || {
            let failed = input.restart().err().map(|e| format!("{e:#}"));
            let _ = handle.send(Command::SourceRestarted(back, generation, failed));
        });
        if !started {
            slot.input.restart_abandoned();
        }
    }

    /// A restart has come back from its thread. Applied only to the instance
    /// it was started for: the source may have been removed and added again
    /// while it ran, and the new one has nothing to do with this result.
    pub(super) fn restarted(&mut self, id: &SourceId, generation: u64, failed: Option<String>) {
        let Some(slot) =
            self.sources.iter_mut().find(|s| &s.input.id == id && s.generation == generation)
        else {
            debug!(source = %id, generation, "a restart finished for a source that has since gone");
            return;
        };
        // The ticks it spent restarting were not a stall.
        slot.stalled_ticks = 0;
        match failed {
            Some(e) => {
                error!(source = %id, e, "restart failed");
                // Armed again, with the backoff every restart has. A source
                // whose restart failed is connecting, not stalled, so the
                // stall sweep never asked again: a screen capture whose start
                // ran past five seconds on 2026-10-03 sat on connecting for
                // twenty minutes until somebody restarted it by hand.
                self.arm_source_restart(id.clone(), "its last restart failed");
            }
            None => debug!(source = %id, "restart finished"),
        }
        self.broadcast_status();
    }

    /// Stop a source's pipeline and whatever its kind holds, on a thread of
    /// its own. The source is already out of `sources` by the time this runs.
    pub(super) fn stop_off_thread(&mut self, input: Arc<InputPipeline>) {
        let id = input.id.clone();
        // Said before the thread starts, so a restart already queued for this
        // source does nothing when its turn comes.
        input.mark_stopping();
        let done = Arc::new(AtomicBool::new(false));
        let finished = done.clone();
        let handle = self.handle.clone();
        let back = id.clone();
        let started = offload::run("stop", id.as_str(), move || {
            input.stop();
            finished.store(true, Ordering::SeqCst);
            let _ = handle.send(Command::SourceStopped(back));
        });
        if started {
            let parked = self.stopping.remove(&id).map(|s| s.parked).unwrap_or_default();
            self.stopping.insert(id, Stopping { done, since: Instant::now(), parked });
        }
    }

    /// Hold an add back while the old pipeline under its id is stopping.
    /// Hands the command back when there is nothing to wait for.
    pub(super) fn park_while_stopping(&mut self, id: &SourceId, cmd: Command) -> Option<Command> {
        match self.stopping.get_mut(id) {
            Some(s) => {
                info!(source = %id, "waiting for the old pipeline under this id to stop before adding it again");
                s.parked.push(cmd);
                None
            }
            None => Some(cmd),
        }
    }

    /// Let go of stops that have finished, and of any that have run past
    /// `STOP_GRACE`, sending on what was parked behind them.
    pub(super) fn release_stopped(&mut self) {
        let now = Instant::now();
        let ready: Vec<SourceId> = self
            .stopping
            .iter()
            .filter(|(_, s)| s.done.load(Ordering::SeqCst) || now - s.since > STOP_GRACE)
            .map(|(id, _)| id.clone())
            .collect();
        for id in ready {
            let Some(s) = self.stopping.remove(&id) else { continue };
            if !s.done.load(Ordering::SeqCst) {
                warn!(
                    source = %id,
                    waited_ms = STOP_GRACE.as_millis() as u64,
                    "the old pipeline under this id has not stopped; going ahead without it"
                );
            }
            for cmd in s.parked {
                if let Err(e) = self.handle.send(cmd) {
                    warn!(source = %id, ?e, "could not send on an add that waited for a stop");
                }
            }
        }
    }

    /// Rebuild an output's pipeline on a thread of its own. The old pipeline
    /// has to reach NULL before the new one is built, and its sink decides how
    /// long that takes.
    pub(super) fn reconnect_off_thread(&mut self, out: Arc<OutputSlot>) {
        if !out.claim_reconnect() {
            debug!(output = %out.id(), "a reconnect of this output is already running");
            return;
        }
        let handle = self.handle.clone();
        let worker = out.clone();
        let started = offload::run("reconnect", out.id().as_str(), move || {
            let failed = worker.reconnect().err().map(|e| format!("{e:#}"));
            let _ = handle.send(Command::OutputReconnected(worker, failed));
        });
        if !started {
            out.reconnect_abandoned();
        }
    }
}
