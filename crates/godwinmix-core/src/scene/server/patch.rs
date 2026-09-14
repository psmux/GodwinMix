//! Patches, not snapshots.
//!
//! `event/scene.patch {seq, source_client, scope, added, updated, removed}` is
//! what every client mirrors off. A patch is computed by comparing the record
//! store before a transaction with the record store after it, which means no
//! command has to remember to describe what it did: a command changes the
//! document and the diff is read off the result. A command that touches
//! nothing produces an empty patch and nothing is sent.
//!
//! The same shape is the undo stack. A patch already carries `before` and
//! `after` for every change, so its inverse is the patch with `added` and
//! `removed` swapped and `before` and `after` swapped, which is why undo is an
//! inverse diff stack and not a pile of whole document copies.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::scene::flat::{FlatDocument, Record};
use crate::scene::id::Id;

/// One record as it was and as it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Update {
    pub before: Record,
    pub after: Record,
}

/// What changed in one transaction.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Patch {
    /// Monotonic, per core. A client that sees a gap asks for a fresh
    /// snapshot rather than guessing.
    pub seq: u64,
    /// Whoever asked for the change, so a client can suppress the echo of its
    /// own edits and not fight its own optimistic drawing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_client: Option<String>,
    /// `document` today. `presence` (who is looking at what) is the other
    /// scope 11 section 4 names and is not implemented.
    pub scope: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub added: Vec<Record>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub updated: Vec<Update>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<Id>,
    /// The records that were removed, kept so the patch can be inverted. Not
    /// on the wire: a client applying a removal only needs the ids.
    #[serde(skip)]
    pub removed_records: Vec<Record>,
    /// What the client called this change, for a label in an undo menu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl Patch {
    /// True when nothing actually changed, which is what an idempotent command
    /// asked to set what is already set produces.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.updated.is_empty() && self.removed.is_empty()
    }

    /// The patch that undoes this one.
    pub fn inverse(&self) -> Patch {
        Patch {
            seq: 0,
            source_client: self.source_client.clone(),
            scope: self.scope.clone(),
            added: self.removed_records.clone(),
            updated: self
                .updated
                .iter()
                .map(|u| Update { before: u.after.clone(), after: u.before.clone() })
                .collect(),
            removed: self.added.iter().map(|r| r.id).collect(),
            removed_records: self.added.clone(),
            label: self.label.clone(),
        }
    }

    /// Fold a later patch into this one, so a drag of forty moves is one step
    /// on the undo stack.
    ///
    /// The result has to read as though the two happened at once: a record
    /// added and then changed is an add of the newer version, a record changed
    /// twice keeps the oldest `before` and the newest `after`, and a record
    /// added and then removed is not in the result at all.
    pub fn merge(&mut self, later: &Patch) {
        self.seq = later.seq.max(self.seq);
        for record in &later.added {
            self.added.push(record.clone());
        }
        for update in &later.updated {
            if let Some(added) = self.added.iter_mut().find(|r| r.id == update.after.id) {
                *added = update.after.clone();
            } else if let Some(mine) = self.updated.iter_mut().find(|u| u.after.id == update.after.id)
            {
                mine.after = update.after.clone();
            } else {
                self.updated.push(update.clone());
            }
        }
        for record in &later.removed_records {
            if let Some(pos) = self.added.iter().position(|r| r.id == record.id) {
                // Added and then removed in the same step: it never existed.
                self.added.remove(pos);
                continue;
            }
            let before = self
                .updated
                .iter()
                .position(|u| u.after.id == record.id)
                .map(|pos| self.updated.remove(pos).before)
                .unwrap_or_else(|| record.clone());
            self.removed.push(record.id);
            self.removed_records.push(before);
        }
        // A record changed and changed back is not a change. Without this, a
        // step that adds a sibling and removes it again leaves an update whose
        // before and after are the same record, and an undo menu shows an
        // entry that does nothing.
        self.updated.retain(|u| u.before != u.after);
    }
}

/// Compare two record stores and say what changed.
///
/// Order in `records` is not meaningful (the fractional `order` field decides
/// it), so the two are compared by id and a reordering shows as an update of
/// the records whose `order` moved, which is exactly the point of fractional
/// indexing: moving one item writes one record.
pub fn diff(before: &FlatDocument, after: &FlatDocument) -> Patch {
    let old: BTreeMap<Id, &Record> = before.records.iter().map(|r| (r.id, r)).collect();
    let new: BTreeMap<Id, &Record> = after.records.iter().map(|r| (r.id, r)).collect();

    let mut patch = Patch { scope: "document".into(), ..Patch::default() };
    for (id, record) in &new {
        match old.get(id) {
            None => patch.added.push((*record).clone()),
            Some(was) if was != record => patch
                .updated
                .push(Update { before: (*was).clone(), after: (*record).clone() }),
            Some(_) => {}
        }
    }
    for (id, record) in &old {
        if !new.contains_key(id) {
            patch.removed.push(*id);
            patch.removed_records.push((*record).clone());
        }
    }
    patch
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::{Canvas, Collection, Content, Item, Scene};
    use crate::scene::flat::Props;

    fn doc() -> Collection {
        let mut c = Collection::new("show", Canvas::default());
        let mut scene = Scene::new("wide");
        scene.items.push(Item::new(Content::Source { source: "cam1".into() }));
        c.scenes.push(scene);
        c
    }

    #[test]
    fn an_unchanged_document_produces_nothing() {
        let a = doc().to_flat();
        assert!(diff(&a, &a).is_empty());
    }

    #[test]
    fn a_moved_item_is_one_updated_record() {
        let mut c = doc();
        let before = c.to_flat();
        c.scenes[0].items[0].transform.position.x = 100.0;
        let patch = diff(&before, &c.to_flat());
        assert_eq!(patch.updated.len(), 1, "moving one item should write one record");
        assert!(patch.added.is_empty() && patch.removed.is_empty());
        let Props::Item(after) = &patch.updated[0].after.props else { panic!("not an item") };
        assert_eq!(after.transform.position.x, 100.0);
    }

    #[test]
    fn a_patch_and_its_inverse_cancel() {
        let mut c = doc();
        let before = c.to_flat();
        c.scenes[0].items[0].opacity = 0.25;
        c.scenes[0].items.push(Item::new(Content::Source { source: "cam2".into() }));
        let after = c.to_flat();
        let patch = diff(&before, &after);
        let back = patch.inverse();
        assert_eq!(back.removed.len(), 1, "the added item is removed again");
        assert_eq!(back.updated.len(), 1);
        let Props::Item(restored) = &back.updated[0].after.props else { panic!("not an item") };
        assert_eq!(restored.opacity, 1.0, "the inverse restores the value that was there");
    }

    #[test]
    fn forty_moves_merge_into_one_step() {
        let mut c = doc();
        let start = c.to_flat();
        let mut merged: Option<Patch> = None;
        let mut previous = start.clone();
        for step in 1..=40 {
            c.scenes[0].items[0].transform.position.x = step as f64;
            let now = c.to_flat();
            let patch = diff(&previous, &now);
            match &mut merged {
                Some(m) => m.merge(&patch),
                None => merged = Some(patch),
            }
            previous = now;
        }
        let merged = merged.expect("forty moves");
        assert_eq!(merged.updated.len(), 1, "a drag is one record, not forty");
        let Props::Item(before) = &merged.updated[0].before.props else { panic!("not an item") };
        let Props::Item(after) = &merged.updated[0].after.props else { panic!("not an item") };
        assert_eq!(before.transform.position.x, 0.0, "the step starts where the drag started");
        assert_eq!(after.transform.position.x, 40.0, "and ends where it ended");
    }

    #[test]
    fn a_record_added_and_removed_in_one_step_never_existed() {
        let mut c = doc();
        let start = c.to_flat();
        c.scenes[0].items.push(Item::new(Content::Source { source: "cam2".into() }));
        let middle = c.to_flat();
        c.scenes[0].items.pop();
        let end = c.to_flat();

        let mut patch = diff(&start, &middle);
        patch.merge(&diff(&middle, &end));
        assert!(patch.is_empty(), "adding and removing the same item is no change: {patch:?}");
    }
}
