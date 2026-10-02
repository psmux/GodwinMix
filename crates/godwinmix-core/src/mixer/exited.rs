//! Restarting a source whose plugin process exited by itself.
//!
//! Before this, a sidecar source whose process was killed went dark for about
//! twelve seconds: two before the stall timer judged it stalled, and ten more
//! of `stall.restart_after_secs` before anything restarted it. Nothing looked
//! at the process. Now every tick asks each source's kind, without waiting,
//! whether its process has exited (`Source::exited`, one `try_wait`), and one
//! that has is handed to the same `arm_source_restart` the stall sweep uses,
//! so the restart in place, the rebuild policy and the generation check are
//! all the ones every other restart goes through.
//!
//! The delay is the existing one, read from `source_attempts`, with one
//! change: the attempt count for an exit comes from the streak of exits (see
//! `plugin::host::exits`), because a plugin that delivers a few frames and
//! dies again is judged live in between, and the live sweep would otherwise
//! put its count back to nothing every time and let it restart at full speed.

use super::Mixer;
use crate::state::SourceId;
use std::time::Instant;
use tracing::warn;

impl Mixer {
    /// One pass over the sources: any whose process exited is restarted.
    /// Costs one `try_wait` per sidecar source per tick and nothing for the
    /// built in kinds.
    pub(super) fn restart_the_exited(&mut self) {
        let exited: Vec<(SourceId, String)> = self
            .sources
            .iter()
            .filter_map(|s| s.input.kind_exited().map(|why| (s.input.id.clone(), why)))
            .collect();
        let now = Instant::now();
        for (id, why) in exited {
            let streak = self.exits.entry(id.clone()).or_default().record(now);
            warn!(source = %id, %why, streak, "the source's plugin process exited; restarting it");
            self.log_timeline(&id, "plugin process exited", true);
            let attempts = self.source_attempts.entry(id.clone()).or_insert(0);
            *attempts = (*attempts).max(streak);
            self.arm_source_restart(id, "its plugin process exited");
        }
    }
}
