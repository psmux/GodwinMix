//! Moving and renaming one file, its converted copy with it.

use super::paths::{clash, inside, join, rename_patiently, name_of, parent_of, plain, rel_path, segment};
use super::Refusal;
use std::path::{Path, PathBuf};

/// One thing that moved, relative to the library and as real paths.
#[derive(Debug, Clone, PartialEq)]
pub struct Moved {
    pub from: String,
    pub to: String,
    pub from_path: PathBuf,
    pub to_path: PathBuf,
    /// Files that went with it: a clip's converted `.web.mp4` copy.
    pub also: Vec<(PathBuf, PathBuf)>,
}

impl Moved {
    pub fn same(rel: &str, path: PathBuf) -> Self {
        Self { from: rel.into(), to: rel.into(), from_path: path.clone(), to_path: path, also: Vec::new() }
    }

    /// Every real path pair this move changed.
    pub fn pairs(&self) -> Vec<(PathBuf, PathBuf)> {
        let mut out = vec![(self.from_path.clone(), self.to_path.clone())];
        out.extend(self.also.iter().cloned());
        out
    }
}

/// The plan for one file, checked but not done: where it is, where it goes.
pub fn plan_file(root: &Path, name: &str, folder: Option<&str>, new_name: Option<&str>) -> Result<Moved, Refusal> {
    let from = rel_path(name, "name")?;
    let real = inside(root, &from, "name")?;
    if !real.is_file() || !crate::media::is_media(&real) {
        return Err(Refusal::missing("name", format!("there is no clip {from:?} in the media library. Call media.list to see the files.")));
    }
    let into = match folder {
        Some(f) => rel_path(f, "folder")?,
        None => parent_of(&from).to_string(),
    };
    let dest = inside(root, &into, "folder")?;
    if !dest.is_dir() || dest.join(crate::gallery::MARKER).is_file() {
        return Err(Refusal::missing("folder", format!("there is no folder {into:?} in the media library. Make it with media.folder.add, or call media.folder.list.")));
    }
    let name = match new_name {
        Some(n) => segment(n, "new_name")?,
        None => name_of(&from).to_string(),
    };
    if !crate::media::is_media(Path::new(&name)) {
        return Err(Refusal::invalid("new_name", format!("{name:?} has no clip or picture extension. Keep the one it has, such as {}.", name_of(&from))));
    }
    let to = join(&into, &name);
    if to == from {
        return Ok(Moved::same(&from, plain(root, &from)));
    }
    let keep = (into == parent_of(&from)).then(|| name_of(&from));
    if let Some(have) = clash(&dest, &name, keep) {
        return Err(Refusal::invalid("new_name", format!("{:?} already has a file called {have:?}. Pick another name, or move that one first.", if into.is_empty() { "The top of the library" } else { &into })));
    }
    let (old, new) = (plain(root, &from), plain(root, &to));
    let mut also = Vec::new();
    let conv = crate::convert::converted_sibling(&old);
    if conv.is_file() && conv != old {
        also.push((conv, crate::convert::converted_sibling(&new)));
    }
    Ok(Moved { from, to, from_path: old, to_path: new, also })
}

/// Do a planned move: the file, then its converted copy.
pub fn move_file(m: &Moved) -> Result<(), Refusal> {
    if m.from == m.to {
        return Ok(());
    }
    rename_patiently(&m.from_path, &m.to_path).map_err(|e| Refusal::io(&format!("moving {}", m.from), e))?;
    for (a, b) in &m.also {
        let _ = rename_patiently(a, b);
    }
    Ok(())
}
