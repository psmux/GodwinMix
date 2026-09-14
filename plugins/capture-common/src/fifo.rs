//! Opening the FIFO an output plugin receives the programme on.
//!
//! An output's media travels the opposite way from a source's, and stdin is
//! already the control channel, so the core makes a FIFO beside the instance's
//! sockets and puts its path in `GMX_MEDIA`. That is written down in
//! `docs/reference/plugin-lifecycle.md`.
//!
//! There is one trap in it, and it is the reason this module exists. Opening a
//! FIFO for reading blocks until somebody opens the other end for writing, and
//! opening it for writing blocks until somebody opens it for reading. The core
//! builds its `filesink` on the FIFO and only then calls `start` on the
//! plugin, so a plugin that waited for `start` before opening its end would
//! deadlock with the core, each waiting for the other.
//!
//! `O_NONBLOCK` breaks it. A read only open with `O_NONBLOCK` returns at once
//! whether or not a writer exists, and clearing the flag afterwards leaves an
//! ordinary blocking descriptor. So the plugin opens its end as soon as it
//! knows the path, which is at `initialize`, and the core's own open then
//! succeeds.

use std::path::Path;

/// A file descriptor this process owns, closed when it is dropped.
#[derive(Debug)]
pub struct Fifo {
    fd: i32,
    path: String,
}

impl Fifo {
    /// The number to give `fdsrc`.
    pub fn fd(&self) -> i32 {
        self.fd
    }

    pub fn path(&self) -> &str {
        &self.path
    }
}

#[cfg(unix)]
impl Drop for Fifo {
    fn drop(&mut self) {
        // SAFETY: the descriptor was opened by `open_read` and nothing else
        // has closed it; `Fifo` is not `Copy` and hands out only the number.
        unsafe {
            libc::close(self.fd);
        }
    }
}

/// Open a FIFO for reading without waiting for a writer.
#[cfg(unix)]
pub fn open_read(path: &Path) -> Result<Fifo, String> {
    use std::os::unix::ffi::OsStrExt;
    let display = path.display().to_string();
    let c = std::ffi::CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("the media path {display} has a nul byte in it"))?;
    // SAFETY: the path is NUL terminated and outlives both calls, and the
    // descriptor is checked before it is used.
    let fd = unsafe { libc::open(c.as_ptr(), libc::O_RDONLY | libc::O_NONBLOCK) };
    if fd < 0 {
        return Err(format!(
            "could not open the programme FIFO at {display}: {}. The core makes it before it \
             starts this plugin, so this usually means the instance was removed.",
            std::io::Error::last_os_error()
        ));
    }
    // Back to a blocking descriptor now that the open has happened. `fdsrc`
    // reads it, and a non blocking read would come back as an error rather
    // than as a wait.
    // SAFETY: `fd` is a descriptor this function just opened.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags >= 0 {
        // SAFETY: as above; clearing one flag on a descriptor we own.
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, flags & !libc::O_NONBLOCK);
        }
    }
    Ok(Fifo { fd, path: display })
}

/// Windows has no FIFO, and the core refuses a sidecar output there before it
/// gets this far. The message is here so a build that somehow reaches it says
/// the same thing the core does.
#[cfg(not(unix))]
pub fn open_read(path: &Path) -> Result<Fifo, String> {
    Err(format!(
        "an output plugin receives the programme on a FIFO and {} has none. The core refuses \
         a sidecar output on this platform for the same reason; a first party output \
         (rtmp/output, srt/output) works everywhere. The named pipe that would fix it is an \
         open question in docs/reference/plugin-lifecycle.md. (wanted {})",
        std::env::consts::OS,
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn a_fifo_opens_with_nobody_writing_yet_and_reads_what_arrives_later() {
        use std::io::Write;
        let dir = std::env::temp_dir().join(format!("gmx-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("media.programme");
        let c = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        // SAFETY: a NUL terminated path this test owns.
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

        // This is the whole point: it returns rather than waiting for a writer.
        let fifo = open_read(&path).expect("the read end opens with no writer");
        assert!(fifo.fd() >= 0);
        assert!(fifo.path().ends_with("media.programme"));

        // And the descriptor is a blocking one that really carries bytes.
        let mut writer = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        writer.write_all(b"programme").unwrap();
        drop(writer);
        let mut buffer = [0u8; 9];
        // SAFETY: reading into a buffer we own from a descriptor we own.
        let read = unsafe { libc::read(fifo.fd(), buffer.as_mut_ptr().cast(), buffer.len()) };
        assert_eq!(read, 9);
        assert_eq!(&buffer, b"programme");

        drop(fifo);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_path_that_is_not_there_says_what_makes_it() {
        let err = open_read(Path::new("/no/such/place/media.programme")).unwrap_err();
        assert!(err.contains("core"), "{err}");
    }
}
