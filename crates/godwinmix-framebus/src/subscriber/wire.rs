//! Talking to the owner: connect, and act on what its socket says.

use std::os::fd::AsRawFd;
use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{Live, Subscriber};
use crate::link::{self, Event, Inbox};
use crate::ring::Ring;
use crate::shm::Region;
use crate::{BusName, Error, Registry};

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

    pub(super) fn dial(&self) -> Result<Live, Error> {
        let stream = UnixStream::connect(&self.path).map_err(|_| Error::NotFound {
            name: self.name.to_string(),
            path: self.path.display().to_string(),
        })?;
        link::write_all(stream.as_raw_fd(), &link::join_record(std::process::id()))?;
        stream.set_nonblocking(true)?;
        Ok(Live { stream: Arc::new(stream), inbox: Inbox::default(), ring: None, reader: 0, last: 0 })
    }

    /// Wait up to `ms` for the socket, then act on what it said.
    pub(super) fn pump(&mut self, ms: i32) -> Result<(), Error> {
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
                    let ring = Arc::new(Ring::attach(region)?);
                    let place = ring.header().readers.get(msg.reader as usize).ok_or_else(|| {
                        Error::Protocol(format!("reader place {} is out of range", msg.reader))
                    })?;
                    place.pid.store(std::process::id(), std::sync::atomic::Ordering::Relaxed);
                    live.ring = Some(ring);
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
