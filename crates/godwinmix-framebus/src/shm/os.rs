//! The system calls under a region: an anonymous descriptor, and mmap.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::NonNull;

use crate::Error;

pub fn page_size() -> usize {
    // SAFETY: sysconf has no preconditions.
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if p > 0 {
        p as usize
    } else {
        4096
    }
}

pub fn os_err(what: &str) -> Error {
    Error::Os(format!("{what}: {}", std::io::Error::last_os_error()))
}

#[cfg(target_os = "linux")]
pub fn anonymous_fd() -> Result<OwnedFd, Error> {
    // SAFETY: a valid C string and flags; the result is checked.
    let fd = unsafe { libc::memfd_create(c"godwinmix-framebus".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(os_err("memfd_create"));
    }
    // SAFETY: fd is a fresh descriptor we own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(not(target_os = "linux"))]
pub fn anonymous_fd() -> Result<OwnedFd, Error> {
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    // macOS allows 31 bytes for the name. pid and a counter make it unique;
    // O_EXCL makes a clash an error rather than a shared object.
    let name = format!(
        "/gmxfb.{}.{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    );
    let c = std::ffi::CString::new(name).expect("no nul in the name");
    let mode: libc::c_uint = 0o600;
    // SAFETY: a valid C string; the result is checked.
    let fd = unsafe { libc::shm_open(c.as_ptr(), libc::O_RDWR | libc::O_CREAT | libc::O_EXCL, mode) };
    if fd < 0 {
        return Err(os_err("shm_open"));
    }
    // SAFETY: the name was just created by us.
    unsafe { libc::shm_unlink(c.as_ptr()) };
    // SAFETY: fd is a fresh descriptor we own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

pub fn map(fd: &OwnedFd, len: usize, offset: usize, write: bool) -> Result<NonNull<u8>, Error> {
    let prot = libc::PROT_READ | if write { libc::PROT_WRITE } else { 0 };
    // SAFETY: fd is a shared memory object at least offset + len long.
    let p = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            len,
            prot,
            libc::MAP_SHARED,
            fd.as_raw_fd(),
            offset as libc::off_t,
        )
    };
    if p == libc::MAP_FAILED {
        return Err(os_err("mmap"));
    }
    Ok(NonNull::new(p.cast()).expect("mmap never returns null on success"))
}

