//! Who owns a name: an advisory lock beside its socket.
//!
//! Two processes that both want to open a camera must agree on which one
//! does, before either opens it, and the agreement must end by itself when
//! the owner dies, however it dies. `flock` on a file in the registry does
//! both: one open file holds it at a time, a second `try_take` from any
//! process (or from another open file in the same process) is refused, and
//! the kernel lets it go when the holder's descriptor closes, which it does
//! for a process killed with SIGKILL. A reader that wants to take over asks
//! again every so often; asking costs one system call and touches nothing
//! the owner holds.

use std::fs::{File, OpenOptions};
use std::os::fd::AsRawFd;
use std::path::PathBuf;

use crate::{BusName, Error, Registry};

/// The right to publish one name. Dropping it lets the next process take it.
#[derive(Debug)]
pub struct Claim {
    name: BusName,
    _file: File,
}

impl Claim {
    /// Take `name` if nobody holds it. `Ok(None)` when somebody does.
    pub fn try_take(registry: &Registry, name: &BusName) -> Result<Option<Claim>, Error> {
        let path = lock_path(registry, name);
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)
            .map_err(|e| Error::Os(format!("opening {}: {e}", path.display())))?;
        // SAFETY: a valid descriptor this function owns.
        let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
        if rc == 0 {
            return Ok(Some(Claim {
                name: name.clone(),
                _file: file,
            }));
        }
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
            return Ok(None);
        }
        Err(Error::Os(format!("locking {}: {err}", path.display())))
    }

    /// Whether somebody holds `name` right now. Takes and drops the claim, so
    /// it is only an answer for this instant.
    pub fn is_held(registry: &Registry, name: &BusName) -> Result<bool, Error> {
        Ok(Claim::try_take(registry, name)?.is_none())
    }

    pub fn name(&self) -> &BusName {
        &self.name
    }
}

/// `camera=<id>.lock` beside `camera=<id>.sock`.
fn lock_path(registry: &Registry, name: &BusName) -> PathBuf {
    let file = name.file_name();
    let stem = file.strip_suffix(".sock").unwrap_or(&file);
    registry.dir().join(format!("{stem}.lock"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry(tag: &str) -> Registry {
        let dir = std::env::temp_dir().join(format!("gmx-claim-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Registry::new(dir).unwrap()
    }

    #[test]
    fn one_holder_at_a_time_and_the_next_takes_it_when_it_goes() {
        let reg = registry("one");
        let name = BusName::camera("cam-wide").unwrap();
        let first = Claim::try_take(&reg, &name).unwrap().expect("nobody held it");
        assert!(Claim::try_take(&reg, &name).unwrap().is_none());
        assert!(Claim::is_held(&reg, &name).unwrap());
        let other = BusName::camera("cam-tight").unwrap();
        assert!(Claim::try_take(&reg, &other).unwrap().is_some(), "names are separate");
        drop(first);
        assert!(Claim::try_take(&reg, &name).unwrap().is_some());
    }
}
