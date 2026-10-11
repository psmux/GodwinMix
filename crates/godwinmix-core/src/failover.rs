//! Backups: which source stands in for which, and when.
//!
//! A source with `on_loss` that has no picture for `after_secs` is drawn as
//! its backup wherever it appears, and comes back when it is steady again or
//! when the operator says. This module is the bookkeeping: the rules, one
//! [`watch::Watch`] per source that has one, and the source each is drawn as
//! right now. It never touches a pipeline; `mixer::failover` reads it and
//! does the drawing.
//!
//! Nothing runs unless asked. A mixer with no rule has an empty board, and
//! every question the mixer asks of an empty board is answered without a
//! loop.

use godwinmix_protocol::backup::OnLoss;
use std::collections::BTreeMap;
use std::time::{Duration, Instant};

mod operator;
pub mod state;
#[cfg(test)]
mod tests;
pub mod watch;

pub use state::Swap;
pub use watch::Step;

/// How long a backup that is on screen may be without a picture before the
/// chain moves on to the next one. Short, because the source behind it is
/// already down; long enough that one late frame does not swap pictures.
pub const BACKUP_GRACE: Duration = Duration::from_secs(2);

/// What the board asks of the mixer about any source.
pub trait Liveness {
    /// Delivering now.
    fn live(&self, id: &str) -> bool;
    /// Gone down: stalled, failed, or connecting again after it had been
    /// live, or not in the mixer at all. A source on its first connection is
    /// neither live nor down.
    fn down(&self, id: &str) -> bool;
    /// Delivered at some point in the last `within`.
    fn live_within(&self, id: &str, within: Duration) -> bool;
}

#[derive(Debug, Default)]
struct Entry {
    /// `None` once the operator has cleared it, which is remembered so a
    /// rebuild of the source does not bring back the rule from its config.
    rule: Option<OnLoss>,
    watch: watch::Watch,
    /// The source drawn in this one's place now, as the mixer applied it.
    showing: Option<String>,
    /// Why the backup is coming off, until the mixer has taken it off.
    leaving: Option<&'static str>,
}

/// Every rule and where each stands.
#[derive(Debug, Default)]
pub struct Board {
    entries: BTreeMap<String, Entry>,
}

impl Board {
    /// A source arrived with this rule in its config. Ignored for a source
    /// the board already knows, so a rebuild keeps what the operator set.
    pub fn adopt(&mut self, id: &str, rule: Option<&OnLoss>) {
        if self.entries.contains_key(id) {
            return;
        }
        if let Some(rule) = rule {
            self.entries.insert(id.to_string(), Entry { rule: Some(rule.clone()), ..Default::default() });
        }
    }

    /// The operator set a rule, or cleared it with `None`. Clearing one that
    /// is on its backup brings the source back.
    pub fn set(&mut self, id: &str, rule: Option<OnLoss>) {
        let entry = self.entries.entry(id.to_string()).or_default();
        if rule.is_none() && entry.watch.engaged() {
            entry.watch.disengage();
            entry.leaving = Some("cleared");
        }
        entry.rule = rule;
    }

    /// The source was removed by somebody. Its rule goes with it.
    pub fn forget(&mut self, id: &str) {
        self.entries.remove(id);
    }

    pub fn rule(&self, id: &str) -> Option<&OnLoss> {
        self.entries.get(id)?.rule.as_ref()
    }

    /// Every source that has a rule, with it.
    pub fn rules(&self) -> Vec<(&str, &OnLoss)> {
        self.entries.iter().filter_map(|(id, e)| Some((id.as_str(), e.rule.as_ref()?))).collect()
    }

    /// True when anything has a rule or is still drawn as a backup. The
    /// mixer asks this first, every tick, and does nothing more when not.
    pub fn busy(&self) -> bool {
        self.entries.values().any(|e| e.rule.is_some() || e.showing.is_some())
    }

    /// The source drawn in `id`'s place, if one is.
    pub fn stand_in(&self, id: &str) -> Option<&str> {
        self.entries.get(id)?.showing.as_deref()
    }

    /// Look at every source with a rule once and move its clock on.
    pub fn step(&mut self, now: Instant, live: &impl Liveness) -> Vec<(String, Step)> {
        let mut steps = Vec::new();
        for (id, entry) in &mut self.entries {
            let Some(rule) = &entry.rule else { continue };
            if let Some(step) = entry.watch.step(rule, live.live(id), live.down(id), now) {
                if step == Step::Return {
                    entry.leaving = Some("returned");
                }
                steps.push((id.clone(), step));
            }
        }
        steps
    }

    /// Where every source with a rule stands, to tell when one has moved.
    pub fn phases(&self) -> Vec<godwinmix_protocol::backup::BackupPhase> {
        self.entries.values().filter_map(|e| Some(e.watch.phase(e.rule.as_ref()?))).collect()
    }

    pub fn engaged(&self, id: &str) -> bool {
        self.entries.get(id).is_some_and(|e| e.watch.engaged())
    }
}
