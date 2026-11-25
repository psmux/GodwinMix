//! Follow and lead: when GodwinMix takes, cut the switchers that follow it;
//! when a switcher that leads cuts, take the mapped source in GodwinMix.
//!
//! The decisions are here, with no IO, so they are tested without a core
//! or a switcher; [`run`] joins them to both.
//!
//! Loops are guarded three ways. A switcher is in one mode, so no one
//! switcher both reads and writes GodwinMix's programme. A move that would
//! change nothing (the switcher already shows the input, GodwinMix already
//! has the source) is not made. And a [`Guard`] counts automatic moves: past
//! a handful in a second something is chasing itself, so sync pauses and
//! says so rather than flicker the programme.

mod guard;
pub mod run;

use std::collections::BTreeMap;

pub use guard::{Guard, Verdict, TRIPPED};

use crate::desk::Desk;
use crate::settings::Mode;

/// What sync needs to know of one switcher at one moment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub id: String,
    pub mode: Mode,
    pub up: bool,
    pub sessions: u32,
    pub program: Option<u16>,
    pub map: BTreeMap<String, u16>,
}

pub fn seen(desk: &Desk) -> Vec<Seen> {
    desk.switchers
        .iter()
        .map(|a| {
            let s = a.snapshot();
            let e = &desk.entries[a.id()];
            Seen { id: s.id, mode: e.mode, up: s.link.is_up(), sessions: s.link.sessions, program: s.state.program(0), map: e.map.clone() }
        })
        .collect()
}

/// GodwinMix took `source`: each following switcher that maps it and does
/// not already show it, with the input to cut to.
pub fn follow(switchers: &[Seen], source: &str) -> Vec<(String, u16)> {
    switchers
        .iter()
        .filter(|s| s.mode == Mode::Follow && s.up)
        .filter_map(|s| s.map.get(source).filter(|i| s.program != Some(**i)).map(|i| (s.id.clone(), *i)))
        .collect()
}

/// What each leading switcher was last seen showing, so only a change acts.
#[derive(Debug, Default)]
pub struct Lead {
    last: BTreeMap<String, (u32, u16)>,
}

impl Lead {
    /// The sources to take: one per leading switcher whose programme moved
    /// to a mapped input since the last look, unless GodwinMix already has
    /// it. A switcher seen for the first time, or on a new session, is only
    /// noted: the dump is where it was, not a cut.
    pub fn changes(&mut self, switchers: &[Seen], mix: Option<&str>) -> Vec<(String, String)> {
        let mut out = Vec::new();
        for s in switchers.iter().filter(|s| s.mode == Mode::Lead && s.up) {
            let Some(now) = s.program else { continue };
            let before = self.last.insert(s.id.clone(), (s.sessions, now));
            let moved = matches!(before, Some((session, input)) if session == s.sessions && input != now);
            if !moved {
                continue;
            }
            if let Some((source, _)) = s.map.iter().find(|(_, i)| **i == now) {
                if mix != Some(source.as_str()) {
                    out.push((s.id.clone(), source.clone()));
                }
            }
        }
        self.last.retain(|id, _| switchers.iter().any(|s| &s.id == id && s.mode == Mode::Lead));
        out
    }
}

#[cfg(test)]
mod tests;
