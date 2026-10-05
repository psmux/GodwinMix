//! Telling a source that plays its own way in and out when it goes on and
//! off the programme.
//!
//! An HTML template has its animation inside the page: it slides its panel
//! in when it is taken and out when it is taken off. The scene does not know
//! that, so after every apply this looks at which sources the programme now
//! shows and calls `cue` on each one that declares `Capability::Cue` and
//! changed. The call only hands a line to the source's own writer thread, and
//! is made with `try_lock`, so a source busy restarting is told on the next
//! tick (twice a second) rather than holding up the mixer.
//!
//! An item leaving with `"exit": {"type": "hold"}` is no longer among the
//! placements, so its source is told `out` at once while the hold keeps it
//! drawn for as long as its way out takes.

use super::slots::Placement;
use super::Mixer;
use crate::plugin::Capability;
use crate::state::SourceId;
use std::collections::HashSet;

/// What one source was told, and which instance of it: a source rebuilt from
/// nothing is a new instance that has been told nothing yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Told {
    instance: usize,
    on_air: bool,
}

impl Mixer {
    /// Cue every source whose place on the programme changed.
    pub(super) fn cue_sources(&mut self, placements: &[Placement]) {
        let shown = shown(placements);
        let mut told = Vec::new();
        for slot in &self.sources {
            let input = &slot.input;
            if !input.capabilities().has(Capability::Cue) {
                continue;
            }
            let now = Told { instance: input.layer().map(|l| std::sync::Arc::as_ptr(&l) as usize).unwrap_or(0), on_air: shown.contains(&input.id) };
            if self.cued.get(&input.id) == Some(&now) {
                continue;
            }
            if input.cue(now.on_air) {
                told.push((input.id.clone(), now));
            }
        }
        for (id, now) in told {
            self.cued.insert(id, now);
        }
        let ids: HashSet<&SourceId> = self.sources.iter().map(|s| &s.input.id).collect();
        self.cued.retain(|id, _| ids.contains(id));
    }
}

/// Every source the placements draw, inside a group too.
fn shown(placements: &[Placement]) -> HashSet<SourceId> {
    let mut out = HashSet::new();
    for p in placements {
        out.insert(p.source.clone());
        out.extend(p.group.iter().map(|c| c.source.clone()));
    }
    out
}
