//! Anonymous shared memory, mapped once and handed to readers as a handle.
//!
//! Linux uses `memfd_create`. macOS has no memfd, so it opens a POSIX shared
//! memory object under a random name and unlinks it at once: from then on the
//! only way to reach it is the descriptor, exactly as with a memfd. Either
//! way nothing is left in the filesystem when every process lets go.
//!
//! The owner maps the whole region read and write. A reader maps the header
//! read and write (it writes its leases there) and the frame data read only,
//! so a reader with a bug cannot scribble on a picture another show is using.

use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::ptr::NonNull;

use crate::Error;

pub struct Region {
    fd: OwnedFd,
    header: NonNull<u8>,
    header_len: usize,
    /// The data mapping; the same as `header` for the owner.
    data: NonNull<u8>,
    data_len: usize,
    split: bool,
}

// SAFETY: the region is plain memory shared between processes; every field
// both sides write to is an atomic, and the owner writes a slot's bytes only
// while no reader holds a lease on it (see ring.rs).
unsafe impl Send for Region {}
unsafe impl Sync for Region {}

pub fn page_size() -> usize {
    // SAFETY: sysconf has no preconditions.
    let p = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if p > 0 {
        p as usize
    } else {
        4096
    }
}

fn os_err(what: &str) -> Error {
    Error::Os(format!("{what}: {}", std::io::Error::last_os_error()))
}

#[cfg(target_os = "linux")]
fn anonymous_fd() -> Result<OwnedFd, Error> {
    // SAFETY: a valid C string and flags; the result is checked.
    let fd = unsafe { libc::memfd_create(c"godwinmix-framebus".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(os_err("memfd_create"));
    }
    // SAFETY: fd is a fresh descriptor we own.
    Ok(unsafe { OwnedFd::from_raw_fd(fd) })
}

#[cfg(not(target_os = "linux"))]
fn anonymous_fd() -> Result<OwnedFd, Error> {
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

fn map(fd: &OwnedFd, len: usize, offset: usize, write: bool) -> Result<NonNull<u8>, Error> {
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

impl Region {
    /// A new zeroed region of `len` bytes, mapped read and write.
    pub fn create(len: usize) -> Result<Region, Error> {
        let fd = anonymous_fd()?;
        // SAFETY: fd is ours; the result is checked.
        if unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) } != 0 {
            return Err(os_err("ftruncate"));
        }
        let p = map(&fd, len, 0, true)?;
        Ok(Region { fd, header: p, header_len: len, data: p, data_len: len, split: false })
    }

    /// Map a region another process made: `header_len` bytes read and write,
    /// the rest read only. Both lengths are whole pages.
    pub fn open(fd: OwnedFd, header_len: usize, total: usize) -> Result<Region, Error> {
        // SAFETY: an fstat on a descriptor we own.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 || (st.st_size as usize) < total {
            return Err(Error::Protocol("the region is smaller than its header says".into()));
        }
        let header = map(&fd, header_len, 0, true)?;
        let data = map(&fd, total - header_len, header_len, false).inspect_err(|_| {
            // SAFETY: header was mapped just above with this length.
            unsafe { libc::munmap(header.as_ptr().cast(), header_len) };
        })?;
        Ok(Region { fd, header, header_len, data, data_len: total - header_len, split: true })
    }

    pub fn fd(&self) -> &OwnedFd {
        &self.fd
    }

    pub fn header_ptr(&self) -> *mut u8 {
        self.header.as_ptr()
    }

    pub fn header_len(&self) -> usize {
        self.header_len
    }

    /// A pointer to byte `offset` of the region, counted from its start.
    pub fn at(&self, offset: usize) -> *mut u8 {
        if self.split {
            debug_assert!(offset >= self.header_len && offset - self.header_len <= self.data_len);
            // SAFETY: bounds are checked by the caller against the header.
            unsafe { self.data.as_ptr().add(offset - self.header_len) }
        } else {
            debug_assert!(offset <= self.data_len);
            // SAFETY: as above.
            unsafe { self.data.as_ptr().add(offset) }
        }
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        // SAFETY: both mappings were made in create or open with these lengths.
        unsafe {
            libc::munmap(self.header.as_ptr().cast(), self.header_len);
            if self.split {
                libc::munmap(self.data.as_ptr().cast(), self.data_len);
            }
        }
    }
}
