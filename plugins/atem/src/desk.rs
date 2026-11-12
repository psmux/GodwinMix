//! Every switcher this plugin talks to, with what GodwinMix knows about each
//! that the switcher does not: its name, its mode and its source map.

use std::collections::BTreeMap;

use godwinmix_atem::{Atem, Error, Switchers, Timing};
use tokio::runtime::Handle;

use crate::settings::{Mode, Settings, SwitcherSpec};

/// What GodwinMix keeps beside one switcher's session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub address: String,
    pub mode: Mode,
    /// GodwinMix source id to ATEM input number.
    pub map: BTreeMap<String, u16>,
    /// The macro last run from here, for the window to light.
    pub last_macro: Option<u16>,
    /// True when it came from the settings, so a settings change may remove
    /// it; a switcher added with `connect` stays until `disconnect`.
    pub from_settings: bool,
}

pub struct Desk {
    pub switchers: Switchers,
    pub entries: BTreeMap<String, Entry>,
    runtime: Handle,
}

impl Desk {
    pub fn new(runtime: Handle, timing: Timing) -> Desk {
        Desk { switchers: Switchers::with_timing(timing), entries: BTreeMap::new(), runtime }
    }

    /// Start a session with a switcher. The link comes up in the background.
    pub fn connect(&mut self, spec: SwitcherSpec, from_settings: bool) -> Result<(), Error> {
        self.switchers.add_on(&self.runtime, &spec.id, &spec.address)?;
        let name = spec.name.unwrap_or_else(|| spec.id.clone());
        let entry = Entry { name, address: spec.address, mode: spec.mode, map: spec.map, last_macro: None, from_settings };
        self.entries.insert(spec.id, entry);
        Ok(())
    }

    pub fn disconnect(&mut self, id: &str) -> Result<(), Error> {
        self.switchers.remove(id)?;
        self.entries.remove(id);
        Ok(())
    }

    /// The named switcher, or the only one when no name is given.
    pub fn pick(&self, id: Option<&str>) -> Result<(&Atem, &Entry), Error> {
        let atem = self.switchers.pick(id)?;
        Ok((atem, &self.entries[atem.id()]))
    }

    pub fn entry_mut(&mut self, id: Option<&str>) -> Result<(String, &mut Entry), Error> {
        let id = self.switchers.pick(id)?.id().to_string();
        let entry = self.entries.get_mut(&id).expect("an entry per switcher");
        Ok((id, entry))
    }

    /// Bring the switchers in line with the settings. One whose address is
    /// unchanged keeps its session; the name, mode and map are taken as
    /// given. Answers the problems, one sentence each.
    pub fn configure(&mut self, settings: Settings) -> Vec<String> {
        let mut problems = settings.problems;
        let wanted: Vec<String> = settings.switchers.iter().map(|s| s.id.clone()).collect();
        let stale: Vec<String> = self
            .entries
            .iter()
            .filter(|(id, e)| e.from_settings && !wanted.contains(id))
            .map(|(id, _)| id.clone())
            .collect();
        for id in stale {
            let _ = self.disconnect(&id);
        }
        for spec in settings.switchers {
            if let Some(e) = self.entries.get_mut(&spec.id).filter(|e| e.address == spec.address) {
                e.name = spec.name.clone().unwrap_or_else(|| spec.id.clone());
                (e.mode, e.map, e.from_settings) = (spec.mode, spec.map, true);
                continue;
            }
            if self.entries.contains_key(&spec.id) {
                let _ = self.disconnect(&spec.id);
            }
            let id = spec.id.clone();
            if let Err(e) = self.connect(spec, true) {
                problems.push(format!("switcher '{id}': {e}"));
            }
        }
        problems
    }

    /// The source a switcher's input stands for, by its map.
    pub fn source_for(entry: &Entry, input: u16) -> Option<&str> {
        entry.map.iter().find(|(_, i)| **i == input).map(|(s, _)| s.as_str())
    }
}
