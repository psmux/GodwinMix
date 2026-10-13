//! Folders: directories under the library, listed, made, moved and removed.

use super::paths::{clash, inside, join, rename_patiently, name_of, parent_of, plain, rel_path, segment};
use super::{Moved, Refusal};
use std::path::Path;

/// What happens to what is inside a folder being removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contents {
    /// Into the folder that held it, or the top for a top level folder.
    Up,
    /// Into the top of the library.
    Top,
}

/// Every folder in the library, `Sermons/2026` style, sorted the way a file
/// manager would. A dotted folder, a link, and a Graphics gallery folder
/// are not listed: none of them is somewhere to keep clips.
pub fn list_folders(root: &Path, max_depth: usize) -> Vec<String> {
    let mut out = Vec::new();
    walk(root, "", max_depth, &mut out);
    out.sort_by_key(|p| p.to_lowercase());
    out
}

fn walk(dir: &Path, rel: &str, depth: usize, out: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Ok(kind) = e.file_type() else { continue };
        if !kind.is_dir() || kind.is_symlink() || name.starts_with('.') || e.path().join(crate::gallery::MARKER).is_file() {
            continue;
        }
        let here = join(rel, &name);
        out.push(here.clone());
        if depth > 0 {
            walk(&e.path(), &here, depth - 1, out);
        }
    }
}

/// Make a folder and any folders above it that are missing. Answers its
/// path and whether it was made now.
pub fn make_folder(root: &Path, raw: &str) -> Result<(String, bool), Refusal> {
    let rel = rel_path(raw, "name")?;
    if rel.is_empty() {
        return Err(Refusal::invalid("name", "a new folder needs a name, such as Sermons or Sermons/2026."));
    }
    let mut made = false;
    let mut at = String::new();
    for part in rel.split('/') {
        let parent = inside(root, &at, "name")?;
        let real = match clash(&parent, part, None) {
            Some(have) if parent.join(&have).is_dir() => have,
            Some(have) => return Err(Refusal::invalid("name", format!("there is a file called {have:?} in {:?} already. Pick another name for the folder.", shown(&at)))),
            None => {
                std::fs::create_dir(parent.join(part)).map_err(|e| Refusal::io("making the folder", e))?;
                made = true;
                part.to_string()
            }
        };
        at = join(&at, &real);
    }
    Ok((at, made))
}

pub(super) fn shown(rel: &str) -> &str {
    if rel.is_empty() { "the top of the library" } else { rel }
}

/// A folder that is there, as a real path, or why not.
pub(super) fn existing(root: &Path, rel: &str, field: &'static str) -> Result<std::path::PathBuf, Refusal> {
    let path = inside(root, rel, field)?;
    if rel.is_empty() || !path.is_dir() || path.join(crate::gallery::MARKER).is_file() {
        return Err(Refusal::missing(field, format!("there is no folder {rel:?} in the media library. Call media.folder.list to see the folders.")));
    }
    Ok(path)
}

/// Move a folder into another (`""` is the top) and give it a new name,
/// either or both. Refused when it would go inside itself.
pub fn move_folder(root: &Path, from: &str, into: Option<&str>, name: Option<&str>) -> Result<Moved, Refusal> {
    let m = plan_folder(root, from, into, name)?;
    if m.from != m.to {
        rename_patiently(&m.from_path, &m.to_path).map_err(|e| Refusal::io("moving the folder", e))?;
    }
    Ok(m)
}

/// A folder's move checked but not done, so a caller can refuse a bad one
/// before it asks anything else.
pub fn plan_folder(root: &Path, from: &str, into: Option<&str>, name: Option<&str>) -> Result<Moved, Refusal> {
    let from = rel_path(from, "name")?;
    existing(root, &from, "name")?;
    let into = match into {
        Some(i) => rel_path(i, "into")?,
        None => parent_of(&from).to_string(),
    };
    if into == from || into.starts_with(&format!("{from}/")) {
        return Err(Refusal::invalid(
            "into",
            format!("{from:?} cannot go inside itself or one of its own folders ({into:?}). Pick a folder outside {from:?}, or move {into:?} out first."),
        ));
    }
    let name = match name {
        Some(n) => segment(n, "to")?,
        None => name_of(&from).to_string(),
    };
    let dest_dir = if into.is_empty() { inside(root, "", "into")? } else { existing(root, &into, "into")? };
    let to = join(&into, &name);
    if to == from {
        return Ok(Moved::same(&from, plain(root, &from)));
    }
    let keep = (into == parent_of(&from)).then(|| name_of(&from));
    if let Some(have) = clash(&dest_dir, &name, keep) {
        return Err(Refusal::invalid("to", format!("{:?} already has something called {have:?}. Pick another name, or move that out of the way first.", shown(&into))));
    }
    let (old, new) = (plain(root, &from), plain(root, &to));
    Ok(Moved { from, to, from_path: old, to_path: new, also: Vec::new() })
}
