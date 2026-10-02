//! A short way to name a registry directory whose own path is too long for a
//! socket address.
//!
//! A Unix socket path is a fixed array in `sockaddr_un`, 104 bytes on macOS
//! and 108 on Linux, and the registry sits under `GODWINMIX_HOME`, which can
//! be as long as a person likes. Past the limit every bus name was refused,
//! so a mixer with a long home had no shared camera and no channel source:
//! `gmxbussrc` failed in a loop with "Could not read from resource".
//!
//! The answer is a symbolic link with a short path that points at the
//! registry: `/tmp/gmx-<uid>/<hash of the directory>`, or the same under
//! `$XDG_RUNTIME_DIR` on Linux. The length limit applies to the address
//! string, not to where it resolves, so a socket bound through the link lands
//! in the registry itself. Nothing else moves: the claims, the listing and
//! every socket stay where they were, and every process computes the same
//! link from the same directory without asking anybody.
//!
//! Unix only. Windows has no frame bus transport yet and no such limit.

use std::path::{Path, PathBuf};

use crate::Error;

/// The link for `dir`, made or mended if need be.
pub(super) fn alias(dir: &Path) -> Result<PathBuf, Error> {
    let dir = std::path::absolute(dir).unwrap_or_else(|_| dir.to_path_buf());
    let base = base()?;
    let link = base.join(format!("{:016x}", fnv1a(dir.as_os_str().as_encoded_bytes())));
    if std::fs::read_link(&link).is_ok_and(|to| to == dir) {
        return Ok(link);
    }
    // A link left pointing elsewhere (a hash collision, or a directory that
    // was moved) is replaced. Two processes doing this at once both end up
    // with the same link, so losing the race is not an error.
    let _ = std::fs::remove_file(&link);
    match std::os::unix::fs::symlink(&dir, &link) {
        Ok(()) => Ok(link),
        Err(_) if std::fs::read_link(&link).is_ok_and(|to| to == dir) => Ok(link),
        Err(e) => Err(Error::Os(format!(
            "linking {} to the frame bus directory {}: {e}",
            link.display(),
            dir.display()
        ))),
    }
}

/// `gmx-<uid>` in a short place this user may write, private to them.
fn base() -> Result<PathBuf, Error> {
    let root = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|d| cfg!(target_os = "linux") && !d.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    // SAFETY: getuid cannot fail.
    let uid = unsafe { libc::getuid() };
    let base = root.join(format!("gmx-{uid}"));
    private_dir(&base, uid)?;
    Ok(base)
}

/// Make `dir` owner only, or check that the one already there is ours. In a
/// shared `/tmp` another user could have made it first, and a link in their
/// directory is one they could point anywhere.
fn private_dir(dir: &Path, uid: u32) -> Result<(), Error> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let _ = std::fs::DirBuilder::new().mode(0o700).create(dir);
    let meta = std::fs::symlink_metadata(dir)
        .map_err(|e| Error::Os(format!("making {}: {e}", dir.display())))?;
    if !meta.is_dir() || meta.uid() != uid || meta.mode() & 0o022 != 0 {
        return Err(Error::Os(format!(
            "{} is not a directory private to this user, so the frame bus will not put a \
             link to its directory there. Remove it, or set GODWINMIX_BUS_DIR to a shorter \
             directory",
            dir.display()
        )));
    }
    Ok(())
}

/// FNV-1a, 64 bits: the same answer in every process and every build, which
/// the standard library's hasher does not promise.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_directory_gets_the_same_short_link_and_it_leads_there() {
        let long = std::env::temp_dir()
            .join(format!("fb-short-{}", std::process::id()))
            .join("a-home-folder-with-a-name-long-enough-to-matter".repeat(2))
            .join("bus");
        std::fs::create_dir_all(&long).unwrap();
        let first = alias(&long).unwrap();
        assert_eq!(alias(&long).unwrap(), first);
        assert!(first.as_os_str().len() < 40, "{}", first.display());
        std::fs::write(long.join("probe"), b"x").unwrap();
        assert_eq!(std::fs::read(first.join("probe")).unwrap(), b"x");
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_dir_all(long.parent().unwrap().parent().unwrap());
    }
}
