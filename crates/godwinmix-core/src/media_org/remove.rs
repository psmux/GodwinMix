//! Removing a folder: what is in it moves out first, then the empty
//! directory goes. No file is ever deleted but the litter a file manager
//! leaves.

use super::folders::{existing, shown, Contents};
use super::paths::{clash, inside, join, parent_of, plain, rel_path, rename_patiently};
use super::{Moved, Refusal};
use std::path::Path;

/// Files a file manager leaves behind, which go with the folder rather than
/// standing in the way of removing it.
const LITTER: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

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
        rename_patiently(&from_path, &to_path).map_err(|e| Refusal::io(&format!("moving {n} out of the folder"), e))?;
        moved.push(Moved { from: from_rel, to: to_rel, from_path, to_path, also: Vec::new() });
    }
    for l in LITTER {
        let _ = std::fs::remove_file(dir.join(l));
    }
    std::fs::remove_dir(&dir).map_err(|e| Refusal::io("removing the empty folder", e))?;
    Ok(moved)
}
