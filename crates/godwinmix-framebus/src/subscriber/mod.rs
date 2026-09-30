//! The reader side. `next` hands back the newest frame the reader has not
//! seen, leased in place, or `None` when nothing new came in time.
//!
//! A reader that is slow simply gets a later frame next time and counts the
//! ones it missed in `Frame::skipped`. When the owner goes away the reader
//! keeps its frames, returns `None` while nobody publishes, and picks the
//! name up again when an owner comes back, even with a different format.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::link::{self, Event, Inbox};
use crate::ring::{Lease, Ring};
use crate::shm::Region;
use crate::{BusName, Error, Layout, Registry};

mod frame;

pub use frame::Frame;
use frame::Returns;

pub struct Subscriber {
    name: BusName,
    path: PathBuf,
    live: Option<Live>,
    returns: Arc<Returns>,
    reconnects: u64,
}

struct Live {
    stream: Arc<UnixStream>,
    inbox: Inbox,
    ring: Option<Arc<Ring>>,
    reader: usize,
    last: u64,
}

impl Subscriber {
    /// Attach to `name`. Refused with `NotFound` when nothing publishes it.
    pub fn connect(registry: &Registry, name: &BusName) -> Result<Subscriber, Error> {
        let path = registry.path(name)?;
        let mut s = Subscriber {
            name: name.clone(),
            path,
            live: None,
            returns: Arc::default(),
            reconnects: 0,
        };
        s.live = Some(s.dial()?);
        let deadline = Instant::now() + Duration::from_secs(2);
        while s.live.as_ref().is_some_and(|l| l.ring.is_none()) && Instant::now() < deadline {
            s.pump(100)?;
        }
        match &s.live {
            Some(l) if l.ring.is_some() => Ok(s),
            _ => Err(Error::Protocol(format!(
                "the owner of {name} closed the connection without handing over its frames; \
                 it has no free reader places. Stop a reader or raise max_readers on the owner"
            ))),
        }
    }

    fn dial(&self) -> Result<Live, Error> {
        let stream = UnixStream::connect(&self.path).map_err(|_| Error::NotFound {
            name: self.name.to_string(),
            path: self.path.display().to_string(),
        })?;
        link::write_all(stream.as_raw_fd(), &link::join_record(std::process::id()))?;
        stream.set_nonblocking(true)?;
        Ok(Live { stream: Arc::new(stream), inbox: Inbox::default(), ring: None, reader: 0, last: 0 })
    }

    pub fn name(&self) -> &BusName {
        &self.name
    }

    /// Connected to a live owner right now.
    pub fn is_connected(&self) -> bool {
        self.live.is_some()
    }

    /// Times the owner went away and this reader found it again.
    pub fn reconnects(&self) -> u64 {
        self.reconnects
    }

    /// The format of the frames `next` hands back, once connected.
    pub fn layout(&self) -> Option<Layout> {
        self.live.as_ref()?.ring.as_ref()?.header().layout()
    }

    /// The newest frame not seen yet, waiting at most `timeout` for one.
    pub fn next(&mut self, timeout: Duration) -> Result<Option<Frame>, Error> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(live) = self.live.as_mut() else {
                if left.is_zero() {
                    return Ok(None);
                }
                std::thread::sleep(left.min(Duration::from_millis(50)));
                if let Ok(l) = self.dial() {
                    self.live = Some(l);
                    self.reconnects += 1;
                }
                continue;
            };
            if let Some(ring) = &live.ring {
                match ring.lease_latest(live.reader, live.last) {
                    Lease::Leased { slot, seq, skipped } => {
                        live.last = seq;
                        return Ok(Some(Frame {
                            ring: ring.clone(),
                            _link: live.stream.clone(),
                            returns: self.returns.clone(),
                            reader: live.reader,
                            slot,
                            seq,
                            skipped,
                        }));
                    }
                    Lease::Full if !left.is_zero() => {
                        self.returns.wait(left.as_millis().min(20) as u64);
                        continue;
                    }
                    Lease::Full | Lease::Nothing => {}
                }
            }
            if left.is_zero() {
                return Ok(None);
            }
            self.pump(left.as_millis().clamp(1, 1000) as i32)?;
        }
    }

    /// Wait up to `ms` for the socket, then act on what it said.
    fn pump(&mut self, ms: i32) -> Result<(), Error> {
        let Some(live) = self.live.as_mut() else { return Ok(()) };
        let fd = live.stream.as_raw_fd();
        if !link::wait_readable(fd, ms)? {
            return Ok(());
        }
        let open = live.inbox.fill(fd).unwrap_or(false);
        loop {
            match live.inbox.pop() {
                Ok(Some(Event::Region(msg, fd))) => {
                    let region = Region::open(fd, msg.header_len as usize, msg.total_len as usize)?;
                    live.ring = Some(Arc::new(Ring::attach(region)?));
                    live.reader = msg.reader as usize;
                    live.last = 0;
                }
                Ok(Some(_)) => {}
                Ok(None) => break,
                Err(e) => return Err(Error::Protocol(e.to_string())),
            }
        }
        if !open {
            self.live = None;
        }
        Ok(())
    }
}
