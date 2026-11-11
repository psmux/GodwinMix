//! VISCA over IP on UDP: each command carries a sequence number, the reply
//! carries it back, and a RESET puts the camera's counter to zero. A lost
//! datagram is sent once more with the same number before giving up.

use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use super::ip::{self, CONTROL, CONTROL_REPLY, REPLY, RESET, SEQ_ABNORMAL};
use super::link::{settle, Link};
use super::reply::{parse, split};
use crate::error::{Code, Error, Result};

pub struct IpLink {
    socket: UdpSocket,
    name: String,
    seq: u32,
    timeout: Duration,
    reset: bool,
}

impl IpLink {
    pub fn connect(to: SocketAddr, timeout: Duration) -> Result<IpLink> {
        let name = format!("the VISCA over IP camera at {to}");
        let any = if to.is_ipv6() { "[::]:0" } else { "0.0.0.0:0" };
        let socket = UdpSocket::bind(any).map_err(|e| Error::io(&name, &e))?;
        socket.connect(to).map_err(|e| Error::io(&name, &e))?;
        Ok(IpLink { socket, name, seq: 0, timeout, reset: false })
    }

    /// RESET: the camera's counter goes to 0, and so does ours. A camera that
    /// does not answer it is used anyway; some never do.
    fn reset(&mut self) -> Result<()> {
        self.reset = true;
        self.seq = 0;
        self.socket.send(&ip::wrap(CONTROL, 0, &RESET)).map_err(|e| Error::io(&self.name, &e))?;
        let until = Instant::now() + self.timeout.min(Duration::from_millis(300));
        while let Some((kind, _, _)) = self.receive(until)? {
            if kind == CONTROL_REPLY {
                break;
            }
        }
        Ok(())
    }

    /// The next datagram before `until`, as its type, number and payload.
    fn receive(&mut self, until: Instant) -> Result<Option<(u16, u32, Vec<u8>)>> {
        let mut buf = [0u8; 512];
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.socket.set_read_timeout(Some(left)).ok();
            match self.socket.recv(&mut buf) {
                Ok(n) => {
                    if let Some((kind, seq, payload)) = ip::unwrap(&buf[..n]) {
                        return Ok(Some((kind, seq, payload.to_vec())));
                    }
                }
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => return Ok(None),
                Err(e) => return Err(Error::io(&self.name, &e)),
            }
        }
    }

    /// One try: send under `seq` and wait for the reply that settles it.
    /// `Ok(None)` when nothing came in time, `Err` with `SEQ` when the camera
    /// says the number was wrong.
    fn attempt(&mut self, kind: u16, seq: u32, message: &[u8], inquiry: bool) -> Result<Option<Vec<u8>>> {
        self.socket.send(&ip::wrap(kind, seq, message)).map_err(|e| Error::io(&self.name, &e))?;
        let until = Instant::now() + self.timeout;
        while let Some((k, s, payload)) = self.receive(until)? {
            if k == CONTROL_REPLY && payload.starts_with(&SEQ_ABNORMAL) {
                return Err(Error::new(Code::BadReply, SEQ));
            }
            if k != REPLY || s != seq {
                continue;
            }
            for m in split(&payload).0 {
                if let Some(done) = parse(&m).and_then(|r| settle(r, inquiry, false, &self.name)) {
                    return done.map(Some);
                }
            }
        }
        Ok(None)
    }
}

const SEQ: &str = "sequence number abnormal";

impl Link for IpLink {
    fn exchange(&mut self, message: &[u8], inquiry: bool) -> Result<Vec<u8>> {
        if !self.reset {
            self.reset()?;
        }
        let kind = if inquiry { ip::INQUIRY } else { ip::COMMAND };
        for _ in 0..2 {
            let seq = self.seq;
            self.seq = self.seq.wrapping_add(1);
            match self.attempt(kind, seq, message, inquiry) {
                Ok(Some(reply)) => return Ok(reply),
                Ok(None) => continue,
                Err(e) if e.message == SEQ => self.reset()?,
                Err(e) => return Err(e),
            }
        }
        Err(Error::new(
            Code::Timeout,
            format!("{} did not answer. Check the address, that VISCA over IP is on in its network menu, and that UDP port {} is not blocked.", self.name, ip::PORT),
        ))
    }

    fn name(&self) -> &str {
        &self.name
    }
}
