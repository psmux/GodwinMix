//! Downstream keys: scenes drawn above whatever is taken.
//!
//! A key is an ordinary scene, a lower third or a logo or a ticker, put on
//! air with `set_key`. Every take after that draws it above the scene or the
//! source taken, in the order the keys went on, until it is switched off, so
//! a graphic that must stay up no longer has to be copied into every scene.
//! The list is in the document (`Collection::keys`) and so survives a
//! restart, and it is not an edit: undo never takes a key off air.

use super::{compose, find, Inner, SceneServer};
use crate::mixer::Placement;
use crate::scene::id::Id;
use anyhow::Result;

/// A key on air, as a listing gives it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct KeyOnAir {
    pub id: Id,
    pub name: String,
}

impl SceneServer {
    /// The keys on air, bottom first. A key whose scene was removed is gone.
    pub fn keys(&self) -> Vec<KeyOnAir> {
        let inner = self.inner.lock();
        inner.keys_on_air()
    }

    /// Put a scene on air as a key, or take it off, and answer with the keys
    /// on air. Putting on one that is on already, or taking off one that is
    /// off, changes nothing.
    pub fn set_key(&self, which: &str, on: bool) -> Result<Vec<KeyOnAir>> {
        let mut inner = self.inner.lock();
        let id = find::scene(&inner.doc, which)?.id;
        let had = inner.doc.keys.contains(&id);
        if on && !had {
            inner.doc.keys.push(id);
        } else if !on && had {
            inner.doc.keys.retain(|k| *k != id);
        }
        if on != had {
            inner.save();
        }
        Ok(inner.keys_on_air())
    }

    /// What the keys draw, above a take of something that is not a scene:
    /// one source taken on its own.
    pub fn key_placements(&self) -> Vec<Placement> {
        self.inner.lock().key_placements(None)
    }

    /// One source full canvas with the keys above it, named after the source
    /// so the mixer still reports that source on air. None while no key is
    /// up, when a source is taken as it always was.
    pub fn source_with_keys(&self, source: &str) -> Option<crate::mixer::ProgramScene> {
        let inner = self.inner.lock();
        let keys = inner.key_placements(None);
        if keys.is_empty() {
            return None;
        }
        let mut placements = vec![Placement::full_canvas(source.to_string(), &inner.canvas)];
        placements.extend(keys);
        Some(crate::mixer::ProgramScene { name: source.to_string(), placements })
    }

    /// Every source the keys draw, for the programme's tally.
    pub fn key_sources(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for p in self.key_placements() {
            if !out.contains(&p.source) {
                out.push(p.source);
            }
        }
        out
    }
}

impl Inner {
    fn keys_on_air(&self) -> Vec<KeyOnAir> {
        self.doc
            .keys
            .iter()
            .filter_map(|id| self.doc.scenes.iter().find(|s| s.id == *id))
            .map(|s| KeyOnAir { id: s.id, name: s.name.clone() })
            .collect()
    }

    /// The keys' placements, bottom first, leaving out the scene being taken
    /// when it is itself a key, which would draw it twice.
    pub(super) fn key_placements(&self, under: Option<Id>) -> Vec<Placement> {
        let mut out = Vec::new();
        for id in self.doc.keys.iter().filter(|k| Some(**k) != under) {
            if let Some(scene) = self.doc.scenes.iter().find(|s| s.id == *id) {
                out.extend(compose::placements(&self.doc, scene, &self.canvas));
            }
        }
        out
    }
}
