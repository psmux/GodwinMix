//! The reader side. `next` hands back the newest frame the reader has not
//! seen, leased in place, or `None` when nothing new came in time.
//!
//! A reader that is slow simply gets a later frame next time and counts the
//! ones it missed in `Frame::skipped`. When the owner goes away the reader
//! keeps its frames, returns `None` while nobody publishes, and picks the
//! name up again when an owner comes back, even with a different format.

use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::link::Inbox;
use crate::ring::{Lease, Ring};
use crate::{BusName, Error, Layout};

mod frame;
mod wire;

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
}
