//! Where a folder picker can start, beyond the mixer's own folders, and how a
//! path is written back to a person.
//!
//! On Windows that is every drive letter, so a recording can go to `D:` when
//! the system drive is small. On Linux and macOS it is `/` and the volumes
//! mounted under the usual folders. Windows also writes every canonical path
//! with the `\\?\` prefix, which nobody types and a tester read as broken; it
//! is taken off whenever the path means the same without it.

use std::path::{Path, PathBuf};

/// The most mounted volumes offered as places. A machine with more is
/// still walked from `/`.
#[cfg_attr(windows, allow(dead_code))]
const MAX_VOLUMES: usize = 32;

/// A path as a person reads and types it.
pub fn shown(path: &Path) -> String {
    simplify(&path.display().to_string())
}

/// `\\?\C:\Users\x` as `C:\Users\x` and `\\?\UNC\nas\share` as
/// `\\nas\share`, when nothing in it needs the prefix: a path of 260
/// characters or more, a device name such as `CON`, or a name ending in a dot
/// or a space all mean something else without it, so those keep it. Any other
/// text is returned as it is, so this is safe to call on every platform.
pub fn simplify(text: &str) -> String {
    let plain = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{rest}")
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        let b = rest.as_bytes();
        if b.len() < 2 || !b[0].is_ascii_alphabetic() || b[1] != b':' || (b.len() > 2 && b[2] != b'\\') {
            return text.to_string();
        }
        rest.to_string()
    } else {
        return text.to_string();
    };
    let names = plain.trim_start_matches('\\').split('\\').skip(1).filter(|n| !n.is_empty());
    let risky = plain.len() >= 260 || plain.contains('/') || names.clone().any(|n| n.ends_with(['.', ' ']) || device(n));
    if risky { text.to_string() } else { plain }
}

/// The names Windows keeps for devices, with or without an extension.
fn device(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or("").trim_end().to_ascii_uppercase();
    matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$")
        || ((stem.starts_with("COM") || stem.starts_with("LPT")) && stem.len() == 4 && stem.as_bytes()[3].is_ascii_digit())
}

/// `D:` and `d:` as `D:\`, since a bare drive letter on Windows means the
/// current folder on that drive rather than its top.
pub fn drive_top(text: &str) -> Option<String> {
    let b = text.as_bytes();
    (b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':').then(|| format!("{}:\\", text[..1].to_ascii_uppercase()))
}

/// The drive letters set in a `GetLogicalDrives` mask, as `C:\`.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn letters(mask: u32) -> Vec<String> {
    (0..26u8).filter(|i| mask & (1 << i) != 0).map(|i| format!("{}:\\", (b'A' + i) as char)).collect()
}

/// Every drive Windows has a letter for, labelled `C:`. Nothing is probed, so
/// an empty card reader or a mapped drive that is offline costs nothing until
/// somebody opens it, and then says why it cannot be read.
#[cfg(windows)]
pub fn system() -> Vec<(String, PathBuf)> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetLogicalDrives() -> u32;
    }
    // SAFETY: takes nothing and returns a bit mask.
    let mask = unsafe { GetLogicalDrives() };
    letters(mask).into_iter().map(|d| (d[..2].to_string(), PathBuf::from(d))).collect()
}

/// `/` as `Computer`, then the volumes mounted where a desktop puts them.
#[cfg(not(windows))]
pub fn system() -> Vec<(String, PathBuf)> {
    let user = std::env::var("USER").unwrap_or_default();
    let mut bases = vec![PathBuf::from("/Volumes")];
    if !user.is_empty() {
        bases.push(Path::new("/media").join(&user));
        bases.push(Path::new("/run/media").join(&user));
    }
    bases.extend([PathBuf::from("/media"), PathBuf::from("/mnt")]);
    let mut out = vec![("Computer".to_string(), PathBuf::from("/"))];
    out.extend(mounted(&bases));
    out
}

/// The folders directly inside each of `bases`, labelled by name, leaving out
/// hidden ones and the bases themselves (`/media/show` is a base, not a
/// volume). At most [`MAX_VOLUMES`]. Windows has drive letters instead.
#[cfg_attr(windows, allow(dead_code))]
pub fn mounted(bases: &[PathBuf]) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for base in bases {
        let Ok(entries) = std::fs::read_dir(base) else { continue };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            if name.starts_with('.') || bases.contains(&path) || !path.is_dir() {
                continue;
            }
            out.push((name, path));
        }
    }
    out.sort_by(|a, b| a.1.cmp(&b.1));
    out.truncate(MAX_VOLUMES);
    out
}
