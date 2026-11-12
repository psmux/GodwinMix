//! Raw VISCA over TCP: the serial bytes on a socket, no header and no
//! sequence number. Replies are matched by order, so anything left over from
//! an earlier command (a preset's late completion) is read off and dropped
//! before each new one. The connection is made on first use and again after
//! it breaks, so a camera powered on after the mixer is picked up.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::{Duration, Instant};

use super::link::{settle, Link};
use super::reply::{parse, split};
use crate::error::{Code, Error, Result};

pub struct TcpLink {
    to: SocketAddr,
    name: String,
    stream: Option<TcpStream>,
    timeout: Duration,
}

impl TcpLink {
    pub fn new(to: SocketAddr, timeout: Duration) -> TcpLink {
        TcpLink { to, name: format!("the VISCA camera at {to} (TCP)"), stream: None, timeout }
    }

    fn stream(&mut self) -> Result<&mut TcpStream> {
        if self.stream.is_none() {
            let s = TcpStream::connect_timeout(&self.to, self.timeout).map_err(|e| Error::io(&self.name, &e))?;
            s.set_nodelay(true).ok();
            self.stream = Some(s);
        }
        Ok(self.stream.as_mut().expect("just made"))
    }

    /// Read off whatever is already waiting, without blocking.
    fn drain(&mut self) -> Result<()> {
        let s = self.stream()?;
        s.set_nonblocking(true).ok();
        let mut buf = [0u8; 256];
        let gone = loop {
            match s.read(&mut buf) {
                Ok(0) => break true,
                Ok(_) => continue,
                Err(_) => break false,
            }
        };
        s.set_nonblocking(false).ok();
        if gone {
            self.stream = None;
        }
        Ok(())
    }

    fn talk(&mut self, message: &[u8], inquiry: bool) -> Result<Vec<u8>> {
        self.drain()?;
        let name = self.name.clone();
        let timeout = self.timeout;
        let s = self.stream()?;
        s.write_all(message).map_err(|e| Error::io(&name, &e))?;
        let until = Instant::now() + timeout;
        let (mut pending, mut buf) = (Vec::new(), [0u8; 256]);
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(Error::new(Code::Timeout, format!("{name} did not answer. Check the port: raw VISCA over TCP is often 5678, 1259 or 5002 depending on the maker.")));
            }
            s.set_read_timeout(Some(left)).ok();
            let n = match s.read(&mut buf) {
                Ok(0) => return Err(Error::new(Code::Unreachable, format!("{name} closed the connection. It will be opened again on the next command."))),
                Ok(n) => n,
                Err(e) => return Err(Error::io(&name, &e)),
            };
            pending.extend_from_slice(&buf[..n]);
            let (messages, rest) = split(&pending);
            pending = rest;
            for m in messages {
                if let Some(done) = parse(&m).and_then(|r| settle(r, inquiry, true, &name)) {
                    return done;
                }
            }
        }
    }
}

impl Link for TcpLink {
    fn exchange(&mut self, message: &[u8], inquiry: bool) -> Result<Vec<u8>> {
        let out = self.talk(message, inquiry);
        if matches!(&out, Err(e) if e.code != Code::Refused) {
            self.stream = None;
        }
        out
    }

    fn name(&self) -> &str {
        &self.name
    }
}
