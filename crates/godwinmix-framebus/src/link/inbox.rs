//! Reading the link: bytes and descriptors in, events out.
//!
//! A stream socket may split or join records however it likes, and a
//! descriptor rides with whichever read picked up the first byte of the
//! record it was sent with. So bytes and descriptors are queued separately,
//! and each region record takes the oldest queued descriptor.

use super::*;

#[derive(Default)]
pub struct Inbox {
    buf: Vec<u8>,
    fds: VecDeque<OwnedFd>,
}

#[cfg(target_os = "linux")]
const RECV_FLAGS: libc::c_int = libc::MSG_CMSG_CLOEXEC | libc::MSG_DONTWAIT;
#[cfg(not(target_os = "linux"))]
const RECV_FLAGS: libc::c_int = libc::MSG_DONTWAIT;

/// Wait up to `timeout_ms` for `sock` to be readable. `true` if it is, or
/// if it closed (the read then says so).
pub fn wait_readable(sock: RawFd, timeout_ms: i32) -> io::Result<bool> {
    let mut p = libc::pollfd { fd: sock, events: libc::POLLIN, revents: 0 };
    // SAFETY: one valid pollfd.
    let n = unsafe { libc::poll(&mut p, 1, timeout_ms) };
    if n < 0 {
        let e = io::Error::last_os_error();
        return if e.kind() == io::ErrorKind::Interrupted { Ok(false) } else { Err(e) };
    }
    Ok(n > 0)
}

impl Inbox {
    /// One non blocking read. `Ok(false)` when the other end closed.
    pub fn fill(&mut self, sock: RawFd) -> io::Result<bool> {
        let mut data = [0u8; 512];
        let mut iov = libc::iovec { iov_base: data.as_mut_ptr().cast(), iov_len: data.len() };
        let mut cbuf = [0u64; 16];
        // SAFETY: msghdr is plain data; zero is a valid starting value.
        let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
        msg.msg_iov = &mut iov;
        msg.msg_iovlen = 1;
        msg.msg_control = cbuf.as_mut_ptr().cast();
        msg.msg_controllen = std::mem::size_of_val(&cbuf) as _;
        // SAFETY: every pointer in msg is to a live local buffer.
        let n = unsafe { libc::recvmsg(sock, &mut msg, RECV_FLAGS) };
        if n < 0 {
            let e = io::Error::last_os_error();
            return match e.kind() {
                io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => Ok(true),
                _ => Err(e),
            };
        }
        self.take_fds(&msg);
        self.buf.extend_from_slice(&data[..n as usize]);
        Ok(n > 0)
    }

    fn take_fds(&mut self, msg: &libc::msghdr) {
        // SAFETY: walking the control buffer the kernel just filled, with the
        // CMSG macros, which stay inside msg_controllen.
        unsafe {
            let mut c = libc::CMSG_FIRSTHDR(msg);
            while !c.is_null() {
                if (*c).cmsg_level == libc::SOL_SOCKET && (*c).cmsg_type == libc::SCM_RIGHTS {
                    let bytes = (*c).cmsg_len as usize - libc::CMSG_LEN(0) as usize;
                    let p = libc::CMSG_DATA(c).cast::<RawFd>();
                    for i in 0..bytes / 4 {
                        let fd = std::ptr::read_unaligned(p.add(i));
                        self.fds.push_back(OwnedFd::from_raw_fd(fd));
                    }
                }
                c = libc::CMSG_NXTHDR(msg, c);
            }
        }
    }

    /// The next whole event in what has been read, if there is one.
    pub fn pop(&mut self) -> io::Result<Option<Event>> {
        let Some(&kind) = self.buf.first() else { return Ok(None) };
        let (len, event) = match kind {
            NUDGE => (1, Some(Event::Nudge)),
            REGION | JOIN if self.buf.len() < RECORD => return Ok(None),
            REGION => {
                let fd = self.fds.pop_front().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidData, "a region record came without its descriptor")
                })?;
                (RECORD, Some(Event::Region(RegionMsg::decode(&self.buf[..RECORD]), fd)))
            }
            JOIN => (RECORD, Some(Event::Join(u32::from_le_bytes(self.buf[4..8].try_into().unwrap())))),
            other => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("unknown frame bus record {other:#x}; the owner and reader are different versions"),
                ))
            }
        };
        self.buf.drain(..len);
        Ok(event)
    }
}

/// The record a reader sends once, right after it connects.
pub fn join_record(pid: u32) -> [u8; RECORD] {
    let mut b = [0u8; RECORD];
    b[0] = JOIN;
    b[4..8].copy_from_slice(&pid.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[test]
    fn records_and_descriptors_arrive_in_order_whatever_the_reads() {
        let (a, b) = UnixStream::pair().unwrap();
        let region = crate::shm::Region::create(4096).unwrap();
        let msg = RegionMsg { reader: 3, header_len: 4096, total_len: 4096, owner_pid: 7 };
        send(a.as_raw_fd(), &[NUDGE, NUDGE], None, true).unwrap();
        send_region(a.as_raw_fd(), &msg, region.fd()).unwrap();
        write_all(a.as_raw_fd(), &join_record(42)).unwrap();
        let mut inbox = Inbox::default();
        let mut got = vec![];
        while got.len() < 4 {
            wait_readable(b.as_raw_fd(), 1000).unwrap();
            assert!(inbox.fill(b.as_raw_fd()).unwrap());
            while let Some(e) = inbox.pop().unwrap() {
                got.push(e);
            }
        }
        assert!(matches!(got[..2], [Event::Nudge, Event::Nudge]));
        assert!(matches!(&got[2], Event::Region(m, _) if *m == msg));
        assert!(matches!(got[3], Event::Join(42)));
        drop(a);
        wait_readable(b.as_raw_fd(), 1000).unwrap();
        assert!(!inbox.fill(b.as_raw_fd()).unwrap(), "a closed socket reads as closed");
    }
}
