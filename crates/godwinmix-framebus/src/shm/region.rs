//! The mapping itself: create one, map one handed over, unmap on drop.

use std::os::fd::{AsRawFd, OwnedFd};
use std::ptr::NonNull;

use super::os::{anonymous_fd, map, os_err};
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

impl Region {
    /// A new zeroed region of `len` bytes, mapped read and write.
    pub fn create(len: usize) -> Result<Region, Error> {
        let fd = anonymous_fd()?;
        // SAFETY: fd is ours; the result is checked.
        if unsafe { libc::ftruncate(fd.as_raw_fd(), len as libc::off_t) } != 0 {
            return Err(os_err("ftruncate"));
        }
        let p = map(&fd, len, 0, true)?;
        Ok(Region {
            fd,
            header: p,
            header_len: len,
            data: p,
            data_len: len,
            split: false,
        })
    }

    /// Map a region another process made: `header_len` bytes read and write,
    /// the rest read only. Both lengths are whole pages.
    pub fn open(fd: OwnedFd, header_len: usize, total: usize) -> Result<Region, Error> {
        // SAFETY: an fstat on a descriptor we own.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd.as_raw_fd(), &mut st) } != 0 || (st.st_size as usize) < total {
            return Err(Error::Protocol(
                "the region is smaller than its header says".into(),
            ));
        }
        let header = map(&fd, header_len, 0, true)?;
        let data = map(&fd, total - header_len, header_len, false).inspect_err(|_| {
            // SAFETY: header was mapped just above with this length.
            unsafe { libc::munmap(header.as_ptr().cast(), header_len) };
        })?;
        Ok(Region {
            fd,
            header,
            header_len,
            data,
            data_len: total - header_len,
            split: true,
        })
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
