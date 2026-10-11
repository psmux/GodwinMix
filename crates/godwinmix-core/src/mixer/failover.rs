//! Drawing a source's backup in its place. See `crate::failover` for the
//! rules; this is where they reach the compositor.
//!
//! The substitution is made where a scene becomes placements
//! (`Mixer::scene_placements`), so it holds in every layout at once: the
//! programme, a scene, a picture in picture, a group. The operator's own
//! take is never changed. A scene without the lost source is untouched by
//! its backup, and a take made while a backup is on is the take that
//! happens, with the stand in still applied inside it.
//!
//! When the swap is on air it runs as a transition, the rule's own, over the
//! same `cross_to` a take uses; off air it is the next apply. A transition
//! the operator started is left to finish: the swap waits for the next tick,
//! half a second at most.

use super::{Mixer, SourceSlot};
use crate::config::SourceConfig;
use crate::failover::{Liveness, Step};
use crate::state::{SourceId, SourceState};
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tracing::warn;

mod commands;
mod draw;
mod tell;

pub use commands::BackupCommand;

/// The mixer's sources, as the board asks about them: the running ones, the
/// ones still having their page probed, which are not down, and the ones
/// that would not start and are being tried again, which are on a first
/// connection that has not come.
pub(super) struct Seen<'a>(pub &'a [SourceSlot], pub &'a [SourceConfig], pub &'a super::unstarted::UnstartedList);

impl Seen<'_> {
    fn slot(&self, id: &str) -> Option<&SourceSlot> {
        self.0.iter().find(|s| s.input.id == id)
    }
}

impl Liveness for Seen<'_> {
    fn live(&self, id: &str) -> bool {
        self.slot(id).is_some_and(|s| matches!(s.input.observed_state(), SourceState::Live))
    }

    fn down(&self, id: &str) -> bool {
        match self.slot(id) {
            Some(s) => match s.input.observed_state() {
                SourceState::Live => false,
                // A try of one that would not start fails at once where a
                // refused connection is answered at once (Linux, macOS), so
                // until it has been live that is still its first connection,
                // given the connect timeout by `unconnected_for`.
                SourceState::Failed if s.watch.live_at.is_none() && self.2.first_tried(id).is_some() => false,
                SourceState::Stalled | SourceState::Failed => true,
                // Connecting again after it had a picture is down; its first
                // connection is not.
                SourceState::Connecting => s.watch.live_at.is_some(),
            },
            // Being rebuilt: down. Being probed, or being tried again after
            // it would not start: a first connection, given the connect
            // timeout by `unconnected_for`.
            None => !self.1.iter().any(|c| c.id == id) && self.2.first_tried(id).is_none(),
        }
    }

    fn live_within(&self, id: &str, within: Duration) -> bool {
        self.live(id) || self.slot(id).is_some_and(|s| s.watch.live_at.is_some_and(|at| at.elapsed() < within))
    }

    /// From its first try, for one that would not start and is tried again,
    /// since each try is a new slot.
    fn unconnected_for(&self, id: &str) -> Option<Duration> {
        let slot = self.slot(id);
        if slot.is_some_and(|s| s.watch.live_at.is_some()) {
            return None;
        }
        if let Some(at) = self.2.first_tried(id) {
            return Some(at.elapsed());
        }
        slot?.watch.added_at.map(|at| at.elapsed())
    }
}

impl Mixer {
    /// The tick's part: move every rule's clock on and draw what it says.
    /// Nothing at all on a mixer with no rule.
    pub(super) fn failover(&mut self, now: Instant) {
        if !self.backups.busy() {
            return;
        }
        self.backups.connect_wait = Duration::from_secs(self.cfg.stall.connect_timeout_secs);
        let before = self.backups.phases();
        let steps = self.backups.step(now, &Seen(&self.sources, &self.pending, &self.unstarted));
        if self.backups.phases() != before {
            // Lost, back, returning: a client draws its countdown from the
            // snapshot, so it hears of each move as it happens.
            self.broadcast_status();
        }
        self.swap_stand_ins();
        for (id, step) in steps {
            if step == Step::Engage {
                self.warn_if_uncovered(&id, now);
            }
        }
    }

    /// Draw every stand in the board asks for, over a transition when one of
    /// them is on air.
    pub(super) fn swap_stand_ins(&mut self) {
        let changes = self.backups.changes(&Seen(&self.sources, &self.pending, &self.unstarted));
        if changes.is_empty() || self.running_transition.is_some() || self.pending_fx.is_some() {
            return;
        }
        let on_air = self.drawn_as_itself();
        let spec = changes
            .iter()
            .find(|c| on_air.contains(&c.source))
            .and_then(|c| self.backup_spec(&c.source));
        let name = spec.as_ref().map(|s| s.kind.name().to_string()).unwrap_or_else(|| "cut".into());
        let leaving = self.current_placements();
        for change in changes {
            let reason = self.backups.shown(&change.source, change.to.clone());
            self.tell_swap(&change, reason, &name);
        }
        match spec.filter(|s| !s.is_cut()) {
            Some(spec) => {
                let id = self.take_generation.fetch_add(1, Ordering::SeqCst) + 1;
                self.settle_transition();
                if let Err(e) = self.cross_to(id, &spec, &leaving) {
                    warn!(?e, "the switch to or from a backup could not run its transition; it was a cut");
                    self.pool.end_transition();
                    self.apply_visibility(true);
                }
            }
            None => self.apply_visibility(true),
        }
        self.redraw_preview();
        self.broadcast_status();
    }

    /// Every source the programme names, before any stand in: the ones whose
    /// swap the audience would see.
    fn drawn_as_itself(&self) -> Vec<SourceId> {
        match &self.program_scene {
            Some(scene) => scene
                .placements
                .iter()
                .flat_map(|p| std::iter::once(p.source.clone()).chain(p.group.iter().map(|c| c.source.clone())))
                .collect(),
            None => self.program_source.iter().cloned().collect(),
        }
    }
}
