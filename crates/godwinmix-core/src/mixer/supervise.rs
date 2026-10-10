//! What the supervisor tick does for a source beyond the stall sweep.
//!
//! The stall sweep only judges a source that has delivered and then stopped.
//! Three other ways for a source to be down went unseen, each found on this
//! machine with a relay whose cable could be pulled:
//!
//! * A source that never delivers. After a restart into a dead relay an RTMP
//!   pull sat on `connecting` for good, because a source that has produced
//!   nothing is not stalled and no error is ever posted. A source that pulls
//!   its feed is now restarted when `stall.connect_timeout_secs` passes with
//!   nothing, on the normal backoff, for as long as it takes.
//! * A restart that never finishes. One took over two minutes inside its
//!   teardown, and while it ran every later restart was refused. After
//!   [`RESTART_ABANDON`] the old pipeline is left to its thread and the
//!   source is built again from nothing beside it.
//! * A plugin that says it is failing. `health` was asked for and never read.
//!   It is now asked every two seconds on a thread of its own, and a
//!   plugin that stays `failing` for `stall.restart_after_secs` is restarted,
//!   as a strike, the way a stall is.
//!
//! It also keeps the time each source was last live, which is what the
//! freeze frame in `Mixer::shows` is measured from.

use super::{Mixer, SourceSlot, FREEZE_HOLD};
use crate::plugin::{Health, PluginState};
use crate::state::{Event, Severity, SourceId, SourceState};
use parking_lot::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod health;
use tracing::warn;

/// How long a restart may run on its worker thread before the mixer stops
/// waiting for it. A working restart takes well under a second; a pull whose
/// client connects inside its state change can take as long as a TCP connect
/// does to give up, which is about twenty.
pub const RESTART_ABANDON: Duration = Duration::from_secs(30);

/// One source's standing with this module.
#[derive(Default)]
pub(super) struct Watch {
    /// When the source was last seen live.
    pub live_at: Option<Instant>,
    asked_at: Option<Instant>,
    asking: Arc<AtomicBool>,
    answer: Arc<Mutex<Option<Health>>>,
    failing_since: Option<Instant>,
    /// What the operator was last told about the plugin's own view.
    told: Option<PluginState>,
    /// A restart is armed because this run had delivered nothing.
    armed_unconnected: bool,
}

impl Mixer {
    /// The part of the tick that is this module's.
    pub(super) fn supervise(&mut self, now: Instant) {
        for slot in &mut self.sources {
            if matches!(slot.input.observed_state(), SourceState::Live) {
                slot.watch.live_at = Some(now);
            }
        }
        self.abandon_hung_restarts();
        self.retry_unconnected();
        self.ask_plugin_health(now);
        self.act_on_plugin_health(now);
    }

    /// Whether the programme draws this source: while it is live, and for up
    /// to `FREEZE_HOLD` after, holding its last frame, if it ever drew one.
    ///
    /// A restart in place keeps that frame on the compositor pad (the flush
    /// it sends stops short of it, `gstutil::stop_flushes_here`), so the hold
    /// costs nothing. Past the hold the slate is the honest answer.
    pub(super) fn shows(&self, slot: &SourceSlot) -> bool {
        if matches!(slot.input.observed_state(), SourceState::Live) {
            return true;
        }
        self.cfg.stall.hold_last_frame
            && slot.input.last_video.seen() > 0
            && slot.watch.live_at.is_some_and(|at| at.elapsed() < FREEZE_HOLD)
    }

    /// Build again from nothing any source whose restart has hung.
    fn abandon_hung_restarts(&mut self) {
        let hung: Vec<(SourceId, Duration)> = self
            .sources
            .iter()
            .filter_map(|s| s.input.restart_running_for().map(|d| (s.input.id.clone(), d)))
            .filter(|(_, d)| *d >= RESTART_ABANDON)
            .collect();
        for (id, running) in hung {
            warn!(
                source = %id,
                running_s = running.as_secs(),
                "a restart of this source has not finished; leaving the old pipeline to its thread and building the source again"
            );
            let _ = self.events.send(Event::Alert {
                severity: Severity::Warning,
                message: format!(
                    "{id} did not come back from a restart in {} s; building it again",
                    running.as_secs()
                ),
                action: None,
            });
            self.rebuild_source(&id);
        }
    }

    /// Restart a pulling source that has delivered nothing since it started.
    fn retry_unconnected(&mut self) {
        let limit = self.cfg.stall.connect_timeout_secs.saturating_mul(1000);
        if limit == 0 {
            return;
        }
        let due: Vec<SourceId> = self
            .sources
            .iter()
            .filter(|s| s.input.pulls_feed() && !s.input.restarting())
            .filter(|s| s.input.health.waiting_ms().is_some_and(|ms| ms >= limit))
            .map(|s| s.input.id.clone())
            .collect();
        for id in due {
            // Refused while one is already armed, so asking on every tick past
            // the deadline schedules exactly one, after the usual backoff.
            if self.arm_source_restart(id.clone(), "it has delivered nothing since it was started") {
                if let Some(slot) = self.sources.iter_mut().find(|s| s.input.id == id) {
                    slot.watch.armed_unconnected = true;
                }
            }
        }
    }

    /// Whether the restart about to run was armed because `id` had delivered
    /// nothing, and it has since come live. The backoff can outlast the
    /// connect it was waiting on: after a cable pull on 2026-10-11 the pull
    /// that had hung in the relay connected at the replug, read live, and was
    /// taken down a second later by the restart armed while it hung.
    pub(super) fn came_live_meanwhile(&mut self, id: &SourceId) -> bool {
        let Some(slot) = self.sources.iter_mut().find(|s| &s.input.id == id) else { return false };
        std::mem::take(&mut slot.watch.armed_unconnected)
            && slot.input.health.waiting_ms().is_none()
            && matches!(slot.input.observed_state(), SourceState::Live)
    }
}
