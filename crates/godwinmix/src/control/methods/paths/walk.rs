//! The filesystem half of `path.list` and `path.create`, with no control
//! plane in it so the tests can run it against a scratch folder.
//!
//! Every path is canonicalised before it is compared, so `..` and a symlink
//! cannot walk out of the places a client may look: the mixer's home folder
//! and the mixer's own folders (the media folder and the folder the config is
//! in). Only folders are listed, never files, and nothing hidden.

use super::{PathEntry, PathListing, PathRoot};
use std::path::{Path, PathBuf};

/// The most folders one answer carries. A folder with more is cut short and
/// says so, which is a picker's problem and never the mixer's.
pub const MAX_ENTRIES: usize = 500;

/// Why a path was not listed or created.
#[derive(Debug, PartialEq)]
pub enum Refusal {
    /// Outside every root. Carries the path asked for.
    Outside(String),
    /// Not there. Carries the path and the nearest folder that is.
    Missing { path: String, nearest: Option<String> },
    /// There, but not a folder, or not readable. Carries the reason.
    Unreadable { path: String, reason: String },
    /// A new folder name that would not stay one folder.
    BadName(String),
}

/// The places a client may look, canonicalised. A root that does not exist is
/// left out: a media folder nobody created yet is somewhere inside home.
pub fn roots(places: &[(&str, Option<PathBuf>)]) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = Vec::new();
    for (label, place) in places {
        let Some(found) = place.as_ref().and_then(|p| std::fs::canonicalize(p).ok()) else {
            continue;
        };
        if found.is_dir() && !out.iter().any(|(_, p)| p == &found) {
            out.push((label.to_string(), found));
        }
    }
    out
}

fn inside(roots: &[(String, PathBuf)], path: &Path) -> bool {
    roots.iter().any(|(_, root)| path.starts_with(root))
}

/// `~` and a relative path both start at the first root, which is home.
fn absolute(roots: &[(String, PathBuf)], asked: Option<&str>) -> PathBuf {
    let home = roots.first().map(|(_, p)| p.clone()).unwrap_or_default();
    match asked.map(str::trim).filter(|s| !s.is_empty() && *s != "~") {
        None => home,
        Some(text) => home.join(text.strip_prefix("~/").unwrap_or(text)),
    }
}

fn nearest(roots: &[(String, PathBuf)], wanted: &Path) -> Option<String> {
    wanted
        .ancestors()
        .skip(1)
        .filter_map(|a| std::fs::canonicalize(a).ok())
        .find(|a| inside(roots, a))
        .map(|a| a.display().to_string())
}

/// List the folders in `asked`, or in home when nothing is asked.
pub fn list(roots: &[(String, PathBuf)], asked: Option<&str>) -> Result<PathListing, Refusal> {
    let wanted = absolute(roots, asked);
    let shown = wanted.display().to_string();
    let path = match std::fs::canonicalize(&wanted) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Refusal::Missing { path: shown, nearest: nearest(roots, &wanted) });
        }
        Err(e) => return Err(Refusal::Unreadable { path: shown, reason: e.to_string() }),
    };
    if !inside(roots, &path) {
        return Err(Refusal::Outside(path.display().to_string()));
    }
    let unreadable = |e: std::io::Error| Refusal::Unreadable { path: path.display().to_string(), reason: e.to_string() };
    let mut dirs = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(unreadable)?.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // A symlink that points out of the roots is not offered, because
        // walking into it would be refused.
        let Ok(real) = std::fs::canonicalize(entry.path()) else { continue };
        if real.is_dir() && inside(roots, &real) {
            dirs.push(PathEntry { name, writable: writable(&real), path: entry.path().display().to_string() });
        }
    }
    dirs.sort_by_key(|d| d.name.to_lowercase());
    let truncated = dirs.len() > MAX_ENTRIES;
    dirs.truncate(MAX_ENTRIES);
    Ok(PathListing {
        writable: writable(&path),
        parent: path.parent().filter(|p| inside(roots, p)).map(|p| p.display().to_string()),
        path: path.display().to_string(),
        dirs,
        truncated,
        roots: roots.iter().map(|(label, p)| PathRoot { label: label.clone(), path: p.display().to_string() }).collect(),
    })
}

/// Make one new folder called `name` in `parent`, and list it. A folder that
/// is already there is listed, so pressing the button twice is harmless.
pub fn create(roots: &[(String, PathBuf)], parent: &str, name: &str) -> Result<PathListing, Refusal> {
    let name = name.trim();
    let bad = name.is_empty()
        || name.len() > 128
        || name.starts_with('.')
        || name.chars().any(|c| matches!(c, '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|'));
    if bad {
        return Err(Refusal::BadName(name.to_string()));
    }
    let listed = list(roots, Some(parent))?;
    let target = Path::new(&listed.path).join(name);
    match std::fs::create_dir(&target) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(Refusal::Unreadable { path: target.display().to_string(), reason: e.to_string() }),
    }
    list(roots, Some(&target.display().to_string()))
}

/// Whether this process may write into `path`: the answer a recording will
/// get, not what the permission bits say about somebody else.
#[cfg(unix)]
pub fn writable(path: &Path) -> bool {
    use std::os::unix::ffi::OsStrExt;
    let Ok(c) = std::ffi::CString::new(path.as_os_str().as_bytes()) else { return false };
    // SAFETY: a valid NUL terminated string, which access only reads.
    unsafe { libc::access(c.as_ptr(), libc::W_OK) == 0 }
}

/// Windows has no cheap access check. The read only attribute is what the
/// metadata says, and a denied write still fails later with its reason.
#[cfg(not(unix))]
pub fn writable(path: &Path) -> bool {
    std::fs::metadata(path).map(|m| !m.permissions().readonly()).unwrap_or(false)
}
