//! The socket between an owner and one reader.
//!
//! Two things travel from owner to reader. A region: a 32 byte record starting
//! `R`, with the region's descriptor attached as `SCM_RIGHTS`. A nudge: the
//! single byte `F`, sent without blocking after every frame. A one byte send
//! either happens whole or not at all, so a full socket buffer can drop nudges
//! but never tears a record; a reader that missed a nudge still reads the
//! newest frame from the header on the next one.
//!
//! A reader sends one record, `J` with its pid, when it connects. After that
//! the owner only reads from the socket to notice it close.

use std::collections::VecDeque;
use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};

pub const REGION: u8 = b'R';
pub const NUDGE: u8 = b'F';
pub const JOIN: u8 = b'J';
pub const RECORD: usize = 32;

/// A region record: which reader place is yours, and how to map the fd.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RegionMsg {
    pub reader: u32,
    pub header_len: u64,
    pub total_len: u64,
    pub owner_pid: u64,
}

impl RegionMsg {
    fn encode(&self) -> [u8; RECORD] {
        let mut b = [0u8; RECORD];
        b[0] = REGION;
        b[4..8].copy_from_slice(&self.reader.to_le_bytes());
        b[8..16].copy_from_slice(&self.header_len.to_le_bytes());
        b[16..24].copy_from_slice(&self.total_len.to_le_bytes());
        b[24..32].copy_from_slice(&self.owner_pid.to_le_bytes());
        b
    }

    fn decode(b: &[u8]) -> RegionMsg {
        let u64_at = |i: usize| u64::from_le_bytes(b[i..i + 8].try_into().unwrap());
        RegionMsg {
            reader: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            header_len: u64_at(8),
            total_len: u64_at(16),
            owner_pid: u64_at(24),
        }
    }
}

#[cfg(target_os = "linux")]
const SEND_FLAGS: libc::c_int = libc::MSG_NOSIGNAL;
#[cfg(not(target_os = "linux"))]
const SEND_FLAGS: libc::c_int = 0;

/// macOS has no `MSG_NOSIGNAL`; the socket option does the same job, so a
/// reader that died never kills the owner with SIGPIPE.
pub fn no_sigpipe(_fd: RawFd) {
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "freebsd"))]
    {
        let one: libc::c_int = 1;
        // SAFETY: a valid option pointer and length on a socket we hold.
        unsafe {
            libc::setsockopt(
                _fd,
                libc::SOL_SOCKET,
                libc::SO_NOSIGPIPE,
                (&one as *const libc::c_int).cast(),
                4,
            )
        };
    }
}

/// Send `bytes`, with `fd` attached when given. Blocking unless the socket is
/// non blocking or `dontwait` is set.
pub fn send(sock: RawFd, bytes: &[u8], fd: Option<RawFd>, dontwait: bool) -> io::Result<usize> {
    let mut iov = libc::iovec {
        iov_base: bytes.as_ptr() as *mut _,
        iov_len: bytes.len(),
    };
    // Room for one descriptor; u64 keeps the buffer aligned for cmsghdr.
    let mut cbuf = [0u64; 8];
    // SAFETY: msghdr is plain data; zero is a valid starting value.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    if let Some(fd) = fd {
        // SAFETY: CMSG_SPACE is a pure size computation; the buffer is big
        // enough for one int, and CMSG_FIRSTHDR points into it.
        unsafe {
            msg.msg_control = cbuf.as_mut_ptr().cast();
            msg.msg_controllen = libc::CMSG_SPACE(4) as _;
            let c = libc::CMSG_FIRSTHDR(&msg);
            (*c).cmsg_level = libc::SOL_SOCKET;
            (*c).cmsg_type = libc::SCM_RIGHTS;
            (*c).cmsg_len = libc::CMSG_LEN(4) as _;
            std::ptr::write_unaligned(libc::CMSG_DATA(c).cast::<RawFd>(), fd);
        }
    }
    let flags = SEND_FLAGS | if dontwait { libc::MSG_DONTWAIT } else { 0 };
    // SAFETY: msg points at live buffers for the duration of the call.
    let n = unsafe { libc::sendmsg(sock, &msg, flags) };
    if n < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(n as usize)
}

pub fn send_region(sock: RawFd, msg: &RegionMsg, fd: &OwnedFd) -> io::Result<()> {
    let rec = msg.encode();
    let n = send(sock, &rec, Some(fd.as_raw_fd()), false)?;
    if n < RECORD {
        // The descriptor went with the first byte; the rest is plain data.
        write_all(sock, &rec[n..])?;
    }
    Ok(())
}

pub fn write_all(sock: RawFd, mut bytes: &[u8]) -> io::Result<()> {
    while !bytes.is_empty() {
        let n = send(sock, bytes, None, false)?;
        bytes = &bytes[n..];
    }
    Ok(())
}

/// What a reader's socket said.
#[derive(Debug)]
pub enum Event {
    Nudge,
    Region(RegionMsg, OwnedFd),
    Join(u32),
}

mod inbox;
pub use inbox::{join_record, wait_readable, Inbox};
