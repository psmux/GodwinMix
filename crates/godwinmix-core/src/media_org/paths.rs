//! Paths inside the library: relative, with `/` on every platform, and
//! made of names every one of Windows, macOS and Linux can hold.

use super::Refusal;
use std::path::{Path, PathBuf};

/// Characters Windows refuses in a name. `/` is the separator here.
const BAD: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*'];
/// Names Windows keeps for devices, with or without an extension.
const RESERVED: &[&str] = &["con", "prn", "aux", "nul"];
/// The longest single name. Every file system here takes 255.
const SEGMENT_MAX: usize = 255;

/// One folder or file name, or why it cannot be one.
pub fn segment(raw: &str, field: &'static str) -> Result<String, Refusal> {
    let name = raw.trim();
    let refuse = |why: String| Refusal::invalid(field, format!("{why} Pick a plain name such as Sermons or Easter 2026."));
    if name.is_empty() {
        return Err(refuse("a name is needed.".into()));
    }
    if name == "." || name == ".." || name.starts_with('.') {
        return Err(refuse(format!("{name:?} starts with a dot, which hides it from the library.")));
    }
    if let Some(c) = name.chars().find(|c| BAD.contains(c) || c.is_control()) {
        return Err(refuse(format!("{name:?} has {c:?} in it, which Windows cannot hold in a name.")));
    }
    if name.ends_with('.') {
        return Err(refuse(format!("{name:?} ends with a dot, which Windows drops.")));
    }
    if name.len() > SEGMENT_MAX {
        return Err(refuse(format!("that name is longer than {SEGMENT_MAX} bytes.")));
    }
    if reserved(name) {
        return Err(refuse(format!("{name:?} is a name Windows keeps for a device.")));
    }
    Ok(name.to_string())
}

fn reserved(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).trim_end().to_lowercase();
    if RESERVED.contains(&stem.as_str()) {
        return true;
    }
    let digit = |s: &str| s.len() == 1 && s.chars().all(|c| c.is_ascii_digit() && c != '0');
    stem.strip_prefix("com").is_some_and(digit) || stem.strip_prefix("lpt").is_some_and(digit)
}

/// A path relative to the library, `Sermons/2026`, checked name by name.
/// `""` is the top. Never absolute, never `..`, never a drive letter: each
/// of those fails one of the name rules.
pub fn rel_path(raw: &str, field: &'static str) -> Result<String, Refusal> {
    let t = raw.trim();
    if t.starts_with('/') || t.contains('\\') {
        return Err(Refusal::invalid(
            field,
            format!("{t:?} is not a path inside the media folder. Give it relative to the media folder with / between names, such as Sermons/2026."),
        ));
    }
    let t = t.trim_end_matches('/');
    if t.is_empty() {
        return Ok(String::new());
    }
    let parts: Result<Vec<String>, Refusal> = t.split('/').map(|p| segment(p, field)).collect();
    Ok(parts?.join("/"))
}

/// The real path of a relative one, confirmed inside the library even
/// through a symlink planted by hand. The last name need not exist yet,
/// which is what a move or a new folder needs; the folder holding it must.
pub fn inside(root: &Path, rel: &str, field: &'static str) -> Result<PathBuf, Refusal> {
    let rel = rel_path(rel, field)?;
    let real_root = root.canonicalize().map_err(|e| Refusal::io("reading the media folder", e))?;
    let mut out = real_root.clone();
    let mut parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let last = parts.pop();
    for p in parts {
        out.push(p);
    }
    let parent = out.canonicalize().map_err(|_| {
        Refusal::missing(field, format!("there is no folder {:?} in the media library. Call media.folder.list to see the folders.", parent_of(&rel)))
    })?;
    if !parent.starts_with(&real_root) {
        return Err(Refusal::invalid(field, format!("{rel:?} leads outside the media folder. Use a folder inside it.")));
    }
    let full = match last {
        Some(l) => parent.join(l),
        None => parent,
    };
    if full.is_symlink() {
        return Err(Refusal::invalid(field, format!("{rel:?} is a link, which the library never follows. Use the real file or folder.")));
    }
    Ok(full)
}

/// The folder part of a relative path: `a/b/c.mp4` gives `a/b`.
pub fn parent_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(p, _)| p).unwrap_or("")
}

/// The last name of a relative path.
pub fn name_of(rel: &str) -> &str {
    rel.rsplit_once('/').map(|(_, n)| n).unwrap_or(rel)
}

/// `folder` joined with `name`, without a leading slash at the top.
pub fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// The path a relative one has under the library as configured, unresolved,
/// which is how `media.list` spells `path` and so how a source names it.
pub fn plain(root: &Path, rel: &str) -> PathBuf {
    let mut out = root.to_path_buf();
    for p in rel.split('/').filter(|p| !p.is_empty()) {
        out.push(p);
    }
    out
}
