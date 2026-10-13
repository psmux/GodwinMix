//! Folders and tags for the media library, kept in one small file beside the
//! clips so the clips themselves are never touched or renamed.
//!
//! The file is `.godwinmix-media.json` in the library's folder. A dotted name
//! keeps it out of the listing. A missing file is an empty one. A damaged one
//! is read as empty too, never as an error, because a library that will not
//! list over a bad sidecar is worse than one that lost its folders; the
//! damaged file is set aside on the next write rather than written over.

mod filter;
mod ops;
mod rules;
#[cfg(test)]
mod tests;

pub use filter::{counts, Filter};
pub use ops::{add_folder, remove_folder, rename_folder, set, Change, Refusal};
pub use rules::{folder_name, tag_list};

use godwinmix_protocol::media::MediaLabel;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::warn;

/// The sidecar's name inside the library folder.
pub const FILE: &str = ".godwinmix-media.json";
/// Where a damaged sidecar goes before a good one is written in its place.
pub const DAMAGED: &str = ".godwinmix-media.damaged.json";

/// Every read, change and write of a sidecar goes through this, so two calls
/// changing labels at once cannot lose one of the changes.
static LOCK: Mutex<()> = Mutex::new(());

/// What the sidecar holds.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Labels {
    /// Every folder, in the order made, so an empty one is kept.
    #[serde(default)]
    pub folders: Vec<String>,
    /// File name as `media.list` gives it, to its folder and tags. A file
    /// with neither is not in here.
    #[serde(default)]
    pub files: BTreeMap<String, MediaLabel>,
}

impl Labels {
    /// A file's folder and tags; empty when it has none.
    pub fn of(&self, name: &str) -> MediaLabel {
        self.files.get(name).cloned().unwrap_or_default()
    }
}

fn path(dir: &Path) -> PathBuf {
    dir.join(FILE)
}

/// Read the sidecar. `true` beside it when a file was there and would not
/// read, so a write knows to set it aside first.
fn read(dir: &Path) -> (Labels, bool) {
    let Ok(text) = std::fs::read_to_string(path(dir)) else { return (Labels::default(), false) };
    match serde_json::from_str::<Labels>(&text) {
        Ok(mut labels) => {
            // A folder a file names but the list lost, from an edit by hand,
            // is listed rather than hidden.
            let named: Vec<String> = labels.files.values().filter_map(|l| l.folder.clone()).collect();
            for f in named {
                if !labels.folders.contains(&f) {
                    labels.folders.push(f);
                }
            }
            (labels, false)
        }
        Err(e) => {
            warn!(dir = %dir.display(), %e, "the media folders and tags file is damaged; reading it as empty");
            (Labels::default(), true)
        }
    }
}

/// Write it whole, under a temporary name first, so a crash mid write leaves
/// the old file rather than half a new one.
fn write(dir: &Path, labels: &Labels, damaged: bool) -> std::io::Result<()> {
    if damaged {
        let _ = std::fs::rename(path(dir), dir.join(DAMAGED));
    }
    let body = serde_json::to_vec_pretty(labels).map_err(std::io::Error::other)?;
    let tmp = dir.join(format!("{FILE}.part"));
    std::fs::write(&tmp, body)?;
    std::fs::rename(&tmp, path(dir))
}

/// The labels, with every file that is no longer on disk dropped. Written
/// back only when something was dropped. Never fails: a sidecar that cannot
/// be read is empty and one that cannot be written is tidied next time.
pub fn load(dir: &Path) -> Labels {
    let _held = LOCK.lock();
    let (mut labels, damaged) = read(dir);
    let before = labels.files.len();
    labels.files.retain(|name, _| dir.join(name).is_file());
    if labels.files.len() != before && !damaged {
        if let Err(e) = write(dir, &labels, false) {
            warn!(dir = %dir.display(), %e, "could not tidy the media folders and tags file");
        }
    }
    labels
}

/// Change the labels under the lock and write them when the change worked
/// and moved something.
pub fn update<T>(dir: &Path, change: impl FnOnce(&mut Labels) -> Result<T, Refusal>) -> Result<T, Refusal> {
    let _held = LOCK.lock();
    let (mut labels, damaged) = read(dir);
    let before = labels.clone();
    let out = change(&mut labels)?;
    labels.files.retain(|_, l| !l.is_empty());
    if labels != before {
        std::fs::create_dir_all(dir).map_err(|e| Refusal::io(dir, e))?;
        write(dir, &labels, damaged).map_err(|e| Refusal::io(dir, e))?;
    }
    Ok(out)
}

/// A file was deleted: its folder and tags go with it.
pub fn forget(dir: &Path, name: &str) {
    let _ = update(dir, |l| {
        l.files.remove(name);
        Ok(())
    });
}
