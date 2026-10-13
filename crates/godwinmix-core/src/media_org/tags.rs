//! The tags file: `.godwinmix-media.json` in the library's folder.
//!
//! A dotted name keeps it out of the listing. A missing file is an empty
//! one. A damaged one reads as empty too, never as an error, because a
//! library that will not list over a bad tags file is worse than one that
//! lost its tags; it is set aside on the next write, not written over.

use super::Refusal;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;
use tracing::warn;

pub const FILE: &str = ".godwinmix-media.json";
/// Where a damaged tags file goes before a good one is written.
pub const DAMAGED: &str = ".godwinmix-media.damaged.json";

/// Every read, change and write goes through this, so two calls changing
/// tags at once cannot lose one of the changes.
static LOCK: Mutex<()> = Mutex::new(());

/// File path relative to the library, `Sermons/easter.mp4`, to its tags. A
/// file with no tags is not in here.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tags {
    #[serde(default)]
    pub files: BTreeMap<String, Vec<String>>,
}

impl Tags {
    pub fn of(&self, name: &str) -> Vec<String> {
        self.files.get(name).cloned().unwrap_or_default()
    }
}

/// The file, and `true` when one was there and would not read.
fn read(dir: &Path) -> (Tags, bool) {
    let Ok(text) = std::fs::read_to_string(dir.join(FILE)) else { return (Tags::default(), false) };
    match serde_json::from_str::<Tags>(&text) {
        Ok(tags) => (tags, false),
        Err(e) => {
            warn!(dir = %dir.display(), %e, "the media tags file is damaged; reading it as empty");
            (Tags::default(), true)
        }
    }
}

/// Written whole under a temporary name first, so a crash mid write leaves
/// the old file rather than half a new one.
fn write(dir: &Path, tags: &Tags, damaged: bool) -> std::io::Result<()> {
    if damaged {
        let _ = std::fs::rename(dir.join(FILE), dir.join(DAMAGED));
    }
    let body = serde_json::to_vec_pretty(tags).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!("{FILE}.part"));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, dir.join(FILE))
}

/// The tags, with every file that is no longer on disk dropped, written
/// back only when something was. Never fails.
pub fn load(dir: &Path) -> Tags {
    let _held = LOCK.lock();
    let (mut tags, damaged) = read(dir);
    let before = tags.files.len();
    tags.files.retain(|name, _| dir.join(name).is_file());
    if tags.files.len() != before && !damaged {
        if let Err(e) = write(dir, &tags, false) {
            warn!(dir = %dir.display(), %e, "could not tidy the media tags file");
        }
    }
    tags
}

/// Change the tags under the lock, and write them when the change worked
/// and moved something.
pub fn update<T>(dir: &Path, change: impl FnOnce(&mut Tags) -> Result<T, Refusal>) -> Result<T, Refusal> {
    let _held = LOCK.lock();
    let (mut tags, damaged) = read(dir);
    let before = tags.clone();
    let out = change(&mut tags)?;
    tags.files.retain(|_, t| !t.is_empty());
    if tags != before {
        write(dir, &tags, damaged).map_err(|e| Refusal::io("saving the tags", e))?;
    }
    Ok(out)
}

/// A file was deleted: its tags go with it.
pub fn forget(dir: &Path, name: &str) {
    let _ = update(dir, |t| {
        t.files.remove(name);
        Ok(())
    });
}

/// A file or a folder moved from `from` to `to`, both relative: its tags,
/// and those of everything under it, follow.
pub fn moved(dir: &Path, from: &str, to: &str) {
    let _ = update(dir, |t| {
        let prefix = format!("{from}/");
        let keys: Vec<String> = t.files.keys().filter(|k| *k == from || k.starts_with(&prefix)).cloned().collect();
        for k in keys {
            let v = t.files.remove(&k).unwrap_or_default();
            t.files.insert(format!("{to}{}", &k[from.len()..]), v);
        }
        Ok(())
    });
}
