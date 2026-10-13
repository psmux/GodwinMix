//! Folders: directories under the library, listed, made, moved and removed.

use super::paths::{clash, inside, join, name_of, parent_of, plain, rel_path, segment};
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

/// Files a file manager leaves behind, which go with the folder rather than
/// standing in the way of removing it.
const LITTER: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

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

fn shown(rel: &str) -> &str {
    if rel.is_empty() { "the top of the library" } else { rel }
}

/// A folder that is there, as a real path, or why not.
fn existing(root: &Path, rel: &str, field: &'static str) -> Result<std::path::PathBuf, Refusal> {
    let path = inside(root, rel, field)?;
    if rel.is_empty() || !path.is_dir() || path.join(crate::gallery::MARKER).is_file() {
        return Err(Refusal::missing(field, format!("there is no folder {rel:?} in the media library. Call media.folder.list to see the folders.")));
    }
    Ok(path)
}

/// Move a folder into another (`""` is the top) and give it a new name,
/// either or both. Refused when it would go inside itself.
pub fn move_folder(root: &Path, from: &str, into: Option<&str>, name: Option<&str>) -> Result<Moved, Refusal> {
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
    std::fs::rename(&old, &new).map_err(|e| Refusal::io("moving the folder", e))?;
    Ok(Moved { from, to, from_path: old, to_path: new, also: Vec::new() })
}

/// Remove a folder, moving everything in it out first. Answers what moved.
/// Refused, with nothing moved, when any of it would land on a name that
/// is already taken where it is going.
pub fn remove_folder(root: &Path, raw: &str, contents: Contents) -> Result<Vec<Moved>, Refusal> {
    let rel = rel_path(raw, "name")?;
    let dir = existing(root, &rel, "name")?;
    let into = if contents == Contents::Up { parent_of(&rel).to_string() } else { String::new() };
    let dest = inside(root, &into, "name")?;
    let mut names: Vec<String> = std::fs::read_dir(&dir).map_err(|e| Refusal::io("reading the folder", e))?.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
    names.retain(|n| !LITTER.contains(&n.as_str()));
    let blocked: Vec<String> = names.iter().filter_map(|n| clash(&dest, n, None).map(|_| join(&into, n))).collect();
    if !blocked.is_empty() {
        return Err(Refusal::invalid(
            "contents",
            format!("{} already holds {}, so what is in {rel:?} cannot move there. Rename or move those first, or choose the other place for what is inside.", shown(&into), blocked.join(", ")),
        )
        .with_valid(blocked));
    }
    let mut moved = Vec::new();
    for n in names {
        let (from_rel, to_rel) = (join(&rel, &n), join(&into, &n));
        let (from_path, to_path) = (plain(root, &from_rel), plain(root, &to_rel));
        std::fs::rename(&from_path, &to_path).map_err(|e| Refusal::io(&format!("moving {n} out of the folder"), e))?;
        moved.push(Moved { from: from_rel, to: to_rel, from_path, to_path, also: Vec::new() });
    }
    for l in LITTER {
        let _ = std::fs::remove_file(dir.join(l));
    }
    std::fs::remove_dir(&dir).map_err(|e| Refusal::io("removing the empty folder", e))?;
    Ok(moved)
}
