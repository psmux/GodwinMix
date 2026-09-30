//! Which instance of a source a piece of delayed work belongs to.
//!
//! Ids are reused. A director removes `hall` and adds it again pointing
//! somewhere else, or a rebuild replaces a page with a fresh copy under the
//! same name. Meanwhile the supervisor may have a retry of the old one asleep
//! on a timer, and a restart of the old one may still be running on its own
//! thread (see `mixer::offload`). Either of those, looked up by id when it
//! comes back, lands on the new source. The recorded session
//! `tests/sessions/source-stall.jsonl` does exactly this: a retry armed for a
//! `hall` that could not connect restarted the `hall` added in its place, a
//! second after it had been taken, and put it back to connecting on air.
//!
//! So every source added gets a generation from a counter that only goes up,
//! the work carries the generation it was started for, and what comes back
//! for a generation that is no longer there is dropped with a line in the
//! debug log. A restart the operator asks for by name still goes to whatever
//! is under that name now, which is what they meant.
//!
//! An output has the same problem and a simpler answer. Its slot is already
//! shared behind an `Arc`, so the worker and the timer carry the slot itself
//! (the timer weakly) and what comes back is matched by identity.

use super::Mixer;
use crate::output::OutputSlot;
use crate::state::SourceId;
use std::sync::{Arc, Weak};
use tracing::debug;

impl Mixer {
    /// The generation for a source about to be added.
    pub(super) fn new_generation(&mut self) -> u64 {
        let g = self.next_generation;
        self.next_generation += 1;
        g
    }

    /// Restart whatever source is under `id` now. Which way it comes back is
    /// its own declaration, not a flag on the core's struct: a kind without
    /// `restart-in-place` is built again from nothing.
    pub(super) fn restart_source(&mut self, id: &SourceId) {
        if self.sources.iter().any(|s| &s.input.id == id && !s.input.restarts_in_place()) {
            self.rebuild_source(id);
        } else {
            self.restart_in_place(id);
        }
    }

    /// A retry the supervisor armed for one generation of `id`. Goes ahead
    /// only if that generation is the one still there.
    pub(super) fn retry_source(&mut self, id: &SourceId, generation: u64) {
        match self.sources.iter().find(|s| &s.input.id == id).map(|s| s.generation) {
            Some(now) if now == generation => self.restart_source(id),
            now => debug!(
                source = %id,
                armed_for = generation,
                now = ?now,
                "dropping a retry armed for a source that has since been removed or replaced"
            ),
        }
    }

    /// The output in place that is `out` itself, if it still is.
    pub(super) fn current_output(&self, out: &Arc<OutputSlot>) -> Option<Arc<OutputSlot>> {
        self.outputs.iter().find(|o| Arc::ptr_eq(o, out)).cloned()
    }

    /// A reconnect the supervisor armed for one output. Goes ahead only if
    /// that output is still the one in place and has a key to connect with.
    pub(super) fn retry_output(&mut self, out: &Weak<OutputSlot>) {
        let Some(out) = out.upgrade().and_then(|o| self.current_output(&o)) else {
            debug!("dropping a reconnect armed for an output that has since been removed or changed");
            return;
        };
        if out.has_key() {
            self.reconnect_off_thread(out);
        }
    }
}
