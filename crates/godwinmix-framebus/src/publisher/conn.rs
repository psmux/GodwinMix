//! One reader's connection, seen from the owner, and the socket chores the
//! bus thread does with it.

use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::link::{self, Event, Inbox, RegionMsg};
use crate::ring::Ring;
use crate::{BusName, Error};

pub struct Conn {
    pub stream: UnixStream,
    pub send: Mutex<()>,
    pub reader: Mutex<usize>,
    pub pid: AtomicU32,
}

/// Send the region record, waiting at most a second for a reader that does
/// not read its socket; such a reader is dropped rather than waited for.
pub fn send_region(c: &Conn, ring: &Ring, reader: usize) -> std::io::Result<()> {
    let _g = c.send.lock().unwrap();
    c.stream.set_nonblocking(false)?;
    c.stream.set_write_timeout(Some(Duration::from_secs(1)))?;
    link::no_sigpipe(c.stream.as_raw_fd());
    let msg = RegionMsg {
        reader: reader as u32,
        header_len: ring.header().data_offset,
        total_len: ring.total_len() as u64,
        owner_pid: std::process::id() as u64,
    };
    let r = link::send_region(c.stream.as_raw_fd(), &msg, ring.region().fd());
    c.stream.set_nonblocking(true)?;
    r
}

/// Read what a reader sent. `false` once it has gone.
pub fn read_reader((c, inbox): &mut (Arc<Conn>, Inbox)) -> bool {
    match inbox.fill(c.stream.as_raw_fd()) {
        Ok(true) => {}
        _ => return false,
    }
    loop {
        match inbox.pop() {
            Ok(Some(Event::Join(pid))) => c.pid.store(pid, Relaxed),
            Ok(Some(_)) => {}
            Ok(None) => return true,
            Err(_) => return false,
        }
    }
}

pub fn poll_all(fds: &[RawFd], timeout_ms: i32) -> Vec<bool> {
    let mut p: Vec<libc::pollfd> = fds
        .iter()
        .map(|&fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        })
        .collect();
    // SAFETY: a valid array of pollfd of the given length.
    let n = unsafe { libc::poll(p.as_mut_ptr(), p.len() as libc::nfds_t, timeout_ms) };
    p.iter().map(|x| n > 0 && x.revents != 0).collect()
}

/// Take `path` for this owner: refused if a live owner answers on it,
/// cleared if the socket was left by one that died.
pub fn claim_path(name: &BusName, path: &std::path::Path) -> Result<(), Error> {
    if !path.exists() {
        return Ok(());
    }
    if std::os::unix::net::UnixStream::connect(path).is_ok() {
        return Err(Error::NameTaken {
            name: name.to_string(),
            pid: 0,
        });
    }
    std::fs::remove_file(path)
        .map_err(|e| Error::Os(format!("removing the stale socket {}: {e}", path.display())))
}
