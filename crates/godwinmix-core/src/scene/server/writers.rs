//! Who changed each record last, so a refusal can name who is in the way.
//!
//! Kept beside the document and updated wherever a patch is made. It only
//! ever names people: whether a step conflicts is decided by comparing states
//! in `conflict.rs`, never by who wrote last.

use std::collections::HashMap;

use super::patch::Patch;
use crate::scene::id::Id;

/// How many recent writers are remembered per record, so a conflict can name
/// the last person who is not the one asking.
const WRITERS_KEPT: usize = 4;

/// How many records' writers are remembered before the list starts again.
const RECORDS_KEPT: usize = 8192;

/// Who changed each record most recently, newest last.
#[derive(Default)]
pub(super) struct Writers {
    by_record: HashMap<Id, Vec<String>>,
    header: Vec<String>,
}

impl Writers {
    /// Note that `client` made this change.
    pub fn note(&mut self, p: &Patch, client: Option<&str>) {
        // Only names are lost by forgetting, never a conflict: the check
        // compares states, and this only says who to blame. A core that has
        // run for days through thousands of added and removed items starts
        // the list again rather than keeping every id it ever saw.
        if self.by_record.len() > RECORDS_KEPT {
            self.by_record.clear();
        }
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
    pub(super) fn other(&self, record: Option<Id>, me: Option<&str>) -> Option<String> {
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
