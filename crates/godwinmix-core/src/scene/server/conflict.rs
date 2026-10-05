//! When undoing your change would overwrite somebody else's.
//!
//! Undo is per client (see `clients.rs`), and a step on one client's stack is
//! an inverse patch: for every record it touches it carries the state the
//! client left the record in (`before`) and the state to put back (`after`).
//! If the record is no longer in the state the client left it in, somebody
//! else has changed it since, and putting the old state back would silently
//! throw their work away.
//!
//! The rule chosen is to refuse rather than to merge. A step is all or
//! nothing: a refusal names every record in the way, who changed each one
//! last, and the two ways on (`force: true`, or editing by hand), and the
//! step stays on the stack. Merging field by field was the alternative, and it
//! was turned down because half an undo is a state neither person asked for,
//! and nobody can see on a phone which half they got.

use std::collections::HashMap;

use schemars::JsonSchema;
use serde::Serialize;

use super::patch::{Header, Patch};
use crate::scene::flat::{FlatDocument, Props, Record};
use crate::scene::id::Id;

/// How many recent writers are remembered per record, so a conflict can name
/// the last person who is not the one asking.
const WRITERS_KEPT: usize = 4;

/// Who changed each record most recently, newest last.
#[derive(Default)]
pub(super) struct Writers {
    by_record: HashMap<Id, Vec<String>>,
    header: Vec<String>,
}

impl Writers {
    /// Note that `client` made this change.
    pub fn note(&mut self, p: &Patch, client: Option<&str>) {
        let who = client.unwrap_or("").to_string();
        let ids = p.added.iter().map(|r| r.id).chain(p.updated.iter().map(|u| u.after.id));
        for id in ids.chain(p.removed.iter().copied()) {
            push(self.by_record.entry(id).or_default(), &who);
        }
        if p.header.is_some() {
            push(&mut self.header, &who);
        }
    }

    /// The most recent writer of a record who is not `me`.
    fn other(&self, record: Option<Id>, me: Option<&str>) -> Option<String> {
        let list = match record {
            Some(id) => self.by_record.get(&id)?,
            None => &self.header,
        };
        let me = me.unwrap_or("");
        list.iter().rev().find(|w| w.as_str() != me).filter(|w| !w.is_empty()).cloned()
    }
}

fn push(list: &mut Vec<String>, who: &str) {
    list.retain(|w| w != who);
    list.push(who.to_string());
    if list.len() > WRITERS_KEPT {
        list.remove(0);
    }
}

/// One record a step cannot put back without overwriting somebody else.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Conflict {
    /// The record. Absent for the collection's own settings, which are not a
    /// record.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub record: Option<Id>,
    /// `item`, `scene` or `collection`.
    pub kind: String,
    /// Its name, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The client id that changed it last, when that is known and is not you.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_by: Option<String>,
    /// True when it, or the scene or group it sat in, has been removed.
    pub gone: bool,
}

impl Conflict {
    fn of(record: &Record, gone: bool, writers: &Writers, me: Option<&str>) -> Conflict {
        let (kind, name) = match &record.props {
            Props::Scene { name, .. } => ("scene", Some(name.clone())),
            Props::Item(item) => ("item", item.name.clone()),
        };
        Conflict {
            record: Some(record.id),
            kind: kind.into(),
            name,
            changed_by: writers.other(Some(record.id), me),
            gone,
        }
    }

    /// `item "stage"`, for a sentence.
    pub fn describe(&self) -> String {
        let what = match (&self.name, self.record) {
            (Some(name), _) => format!("{} {name:?}", self.kind),
            (None, Some(id)) => format!("{} {id}", self.kind),
            (None, None) => "the collection's settings".into(),
        };
        let how = if self.gone { "removed" } else { "changed" };
        match &self.changed_by {
            Some(who) => format!("{what} ({how} by {who})"),
            None => format!("{what} ({how} since)"),
        }
    }
}

/// Every record `step` would overwrite somebody else's work on.
pub(super) fn check(
    now: &FlatDocument,
    step: &Patch,
    writers: &Writers,
    me: Option<&str>,
) -> Vec<Conflict> {
    let current: HashMap<Id, &Record> = now.records.iter().map(|r| (r.id, r)).collect();
    let mut out = Vec::new();
    // A record the step changes back has to be as this client left it.
    for u in &step.updated {
        let is = current.get(&u.before.id);
        if is != Some(&&u.before) {
            out.push(Conflict::of(&u.before, is.is_none(), writers, me));
        }
    }
    // A record the step removes (this client added it) may have been edited
    // since. Already gone is fine: there is nothing left to remove.
    for r in &step.removed_records {
        if current.get(&r.id).is_some_and(|is| *is != r) {
            out.push(Conflict::of(r, false, writers, me));
        }
    }
    // A record the step puts back (this client removed it) needs somewhere
    // to go: its scene or group must still exist, or be coming back with it.
    for r in &step.added {
        let Some(parent) = r.parent else { continue };
        let returning = step.added.iter().any(|a| a.id == parent);
        if !current.contains_key(&parent) && !returning {
            out.push(Conflict::of(r, true, writers, me));
        }
    }
    if let Some(h) = &step.header {
        if Header::of(now) != h.before {
            out.push(Conflict {
                record: None,
                kind: "collection".into(),
                name: None,
                changed_by: writers.other(None, me),
                gone: false,
            });
        }
    }
    out
}

/// Everything a patch changed, as the same list, for a draft refused because
/// its scene moved on.
pub(super) fn changes(since: &Patch, writers: &Writers, me: Option<&str>) -> Vec<Conflict> {
    let added = since.added.iter().map(|r| Conflict::of(r, false, writers, me));
    let updated = since.updated.iter().map(|u| Conflict::of(&u.after, false, writers, me));
    let removed = since.removed_records.iter().map(|r| Conflict::of(r, true, writers, me));
    added.chain(updated).chain(removed).collect()
}

/// The step without the records in `conflicts`, for an abort and for a forced
/// undo of a record that has nowhere left to go.
pub(super) fn without(step: &Patch, conflicts: &[Conflict]) -> Patch {
    let skip = |id: &Id| conflicts.iter().any(|c| c.record == Some(*id));
    let mut out = step.clone();
    out.updated.retain(|u| !skip(&u.before.id));
    out.removed.retain(|id| !skip(id));
    out.removed_records.retain(|r| !skip(&r.id));
    out.added.retain(|r| !skip(&r.id));
    if conflicts.iter().any(|c| c.record.is_none()) {
        out.header = None;
    }
    out
}

/// An undo or redo refused because it would overwrite somebody else's change.
#[derive(Debug, Clone)]
pub struct Refused {
    /// `undo` or `redo`, the method's own word.
    pub verb: &'static str,
    pub conflicts: Vec<Conflict>,
}

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let list: Vec<String> = self.conflicts.iter().map(Conflict::describe).collect();
        write!(
            f,
            "your last change cannot be {}ne without overwriting somebody else's work on {}. \
             Nothing was {}ne and the step stays on your stack. Send scene.{} with force: true \
             to put your version back over theirs, or change it by hand.",
            self.verb,
            list.join(", "),
            self.verb,
            self.verb
        )
    }
}

impl std::error::Error for Refused {}
