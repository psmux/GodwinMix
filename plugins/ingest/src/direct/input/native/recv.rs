//! Taking datagrams off the socket a batch at a time: `recvmmsg` on Linux,
//! one system call for up to [`BATCH`] datagrams; elsewhere one blocking
//! receive and then whatever else is already waiting, without blocking.

use std::io;

use socket2::Socket;

/// Datagrams per batch: at 40 Mbit/s of 1316 byte datagrams, about 8 ms.
pub const BATCH: usize = 32;
/// Room for one datagram. TS over UDP is seven packets, 1316 bytes, or
/// 1328 with RTP; anything longer is counted as junk.
pub const SLOT: usize = 2048;

pub struct Batch {
    data: Vec<u8>,
    lens: [usize; BATCH],
    pub count: usize,
}

impl Default for Batch {
    fn default() -> Batch {
        Batch { data: vec![0; BATCH * SLOT], lens: [0; BATCH], count: 0 }
    }
}

impl Batch {
    pub fn get(&self, i: usize) -> &[u8] {
        &self.data[i * SLOT..i * SLOT + self.lens[i]]
    }

    /// Fill the batch from `s`, waiting up to the socket's timeout for the
    /// first datagram. `Ok(0)` when nothing came.
    pub fn fill(&mut self, s: &Socket) -> io::Result<usize> {
        self.count = 0;
        match fill(self, s) {
            Ok(n) => Ok(n),
            Err(e) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut | io::ErrorKind::Interrupted) => Ok(0),
            Err(e) => Err(e),
        }
    }
}

#[cfg(target_os = "linux")]
fn fill(b: &mut Batch, s: &Socket) -> io::Result<usize> {
    use std::os::fd::AsRawFd;
    // SAFETY: every iovec points into `b.data` at its own slot, and every
    // header at its own iovec; both arrays outlive the call.
    unsafe {
        let mut iov: [libc::iovec; BATCH] = std::mem::zeroed();
        let mut hdr: [libc::mmsghdr; BATCH] = std::mem::zeroed();
        for i in 0..BATCH {
            iov[i] = libc::iovec { iov_base: b.data.as_mut_ptr().add(i * SLOT).cast(), iov_len: SLOT };
            hdr[i].msg_hdr.msg_iov = &mut iov[i];
            hdr[i].msg_hdr.msg_iovlen = 1;
        }
        let n = libc::recvmmsg(s.as_raw_fd(), hdr.as_mut_ptr(), BATCH as u32, libc::MSG_WAITFORONE, std::ptr::null_mut());
        if n < 0 {
            return Err(io::Error::last_os_error());
        }
        for (i, h) in hdr.iter().take(n as usize).enumerate() {
            b.lens[i] = h.msg_len as usize;
        }
        b.count = n as usize;
        Ok(b.count)
    }
}

#[cfg(not(target_os = "linux"))]
fn fill(b: &mut Batch, s: &Socket) -> io::Result<usize> {
    let first = one(b, s, 0, false)?;
    b.lens[0] = first;
    b.count = 1;
    while b.count < BATCH && cfg!(unix) {
        match one(b, s, b.count, true) {
            Ok(n) => {
                b.lens[b.count] = n;
                b.count += 1;
            }
            Err(_) => break,
        }
    }
    Ok(b.count)
}

/// One datagram into slot `i`; `now` does not wait.
#[cfg(not(target_os = "linux"))]
fn one(b: &mut Batch, s: &Socket, i: usize, now: bool) -> io::Result<usize> {
    let slot = &mut b.data[i * SLOT..(i + 1) * SLOT];
    // SAFETY: the slot is initialised bytes, which MaybeUninit<u8> may view.
    let buf = unsafe { &mut *(slot as *mut [u8] as *mut [std::mem::MaybeUninit<u8>]) };
    #[cfg(unix)]
    if now {
        return s.recv_with_flags(buf, libc::MSG_DONTWAIT);
    }
    let _ = now;
    s.recv(buf)
}
