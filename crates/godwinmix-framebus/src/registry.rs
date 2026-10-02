//! Finding a bus by name.
//!
//! The registry is a directory with one socket per published name in it. The
//! station owns the directory and gives its path to every show it starts (as
//! `GODWINMIX_BUS_DIR`); an owner binds its socket there and a reader looks
//! its name up there. Nothing else is shared, so a show on its own, with no
//! station, finds buses the same way.

use std::path::{Path, PathBuf};

use crate::{BusName, Error};

#[cfg(unix)]
mod short;

/// The environment variable a station sets for the shows it starts.
pub const DIR_ENV: &str = "GODWINMIX_BUS_DIR";

/// A Unix socket path longer than this does not fit `sockaddr_un` on macOS
/// (104 bytes with the nul); Linux allows 108.
const MAX_SOCKET_PATH: usize = 103;

#[derive(Clone, Debug)]
pub struct Registry {
    dir: PathBuf,
}

impl Registry {
    /// A registry in `dir`, made (owner only, 0700) if it does not exist. An
    /// existing directory keeps the permissions it has.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Registry, Error> {
        let dir = dir.into();
        if dir.is_dir() {
            return Ok(Registry { dir });
        }
        std::fs::create_dir_all(&dir).map_err(|e| {
            Error::Os(format!(
                "making the frame bus directory {}: {e}",
                dir.display()
            ))
        })?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Registry { dir })
    }

    /// The registry the station set, or this user's default one.
    pub fn from_env() -> Result<Registry, Error> {
        Registry::new(Registry::default_dir())
    }

    /// `GODWINMIX_BUS_DIR`, else `$XDG_RUNTIME_DIR/godwinmix/bus` on Linux,
    /// else `godwinmix-<uid>/bus` in the temporary directory (on macOS that is
    /// already private to the user).
    pub fn default_dir() -> PathBuf {
        if let Some(d) = std::env::var_os(DIR_ENV).filter(|d| !d.is_empty()) {
            return d.into();
        }
        if cfg!(target_os = "linux") {
            if let Some(d) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| !d.is_empty()) {
                return Path::new(&d).join("godwinmix").join("bus");
            }
        }
        std::env::temp_dir()
            .join(format!("godwinmix-{}", user_id()))
            .join("bus")
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The address of `name`'s socket. In the directory itself when that
    /// fits a socket address; otherwise through a short link to the
    /// directory (see `short`), so a long `GODWINMIX_HOME` still works.
    /// Refused only when even that is too long.
    pub fn path(&self, name: &BusName) -> Result<PathBuf, Error> {
        let path = self.dir.join(name.file_name());
        let len = path.as_os_str().len();
        if len <= MAX_SOCKET_PATH {
            return Ok(path);
        }
        #[cfg(unix)]
        {
            let short = short::alias(&self.dir)?.join(name.file_name());
            if short.as_os_str().len() <= MAX_SOCKET_PATH {
                return Ok(short);
            }
        }
        Err(Error::BadName(format!(
            "the socket for {name} would be {} ({len} bytes), and a Unix socket path \
             can be at most {MAX_SOCKET_PATH}, even through a short link to the directory. \
             Use a shorter id, or point {DIR_ENV} at a shorter directory",
            path.display()
        )))
    }

    /// Every name with a socket in the directory. A socket left by an owner
    /// that died is listed until the next owner of that name replaces it; a
    /// subscribe to it answers `NotFound`.
    pub fn list(&self) -> Vec<BusName> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return vec![];
        };
        let mut names: Vec<BusName> = entries
            .filter_map(|e| BusName::from_file_name(e.ok()?.file_name().to_str()?))
            .collect();
        names.sort_by_key(|n| n.to_string());
        names
    }
}

#[cfg(unix)]
fn user_id() -> u32 {
    // SAFETY: getuid cannot fail.
    unsafe { libc::getuid() }
}

#[cfg(not(unix))]
fn user_id() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "user".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_long_directory_is_reached_through_a_short_address() {
        let dir = PathBuf::from(format!("/tmp/fb-long-{}/{}", std::process::id(), "d".repeat(100)));
        let r = Registry::new(&dir).unwrap();
        let path = r.path(&BusName::camera("cam").unwrap()).unwrap();
        assert!(path.as_os_str().len() <= MAX_SOCKET_PATH, "{}", path.display());
        assert!(!path.starts_with(&dir));
        let _ = std::fs::remove_dir_all(dir.parent().unwrap());
    }

    #[test]
    fn a_name_too_long_even_for_the_short_address_is_refused_with_the_way_out() {
        let r = Registry { dir: PathBuf::from(format!("/tmp/{}", "d".repeat(100))) };
        let name = BusName::channel(&"a".repeat(64), &"b".repeat(64)).unwrap();
        let e = r.path(&name).unwrap_err();
        assert!(e.to_string().contains(DIR_ENV), "{e}");
    }
}
