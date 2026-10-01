//! What one HLS output reports, written by its packager's thread and read
//! by `show.list` and `show.stats`. The lock is held for an assignment.

use godwinmix_protocol::destination::{DestinationLive, DestinationState};
use parking_lot::Mutex;
use std::time::{Duration, Instant};

pub struct Board {
    inner: Mutex<Inner>,
}

struct Inner {
    live: DestinationLive,
    changed: Instant,
    window: Instant,
    bytes: u64,
}

impl Default for Board {
    fn default() -> Board {
        let now = Instant::now();
        let live = DestinationLive { state: DestinationState::Waiting, ..Default::default() };
        Board { inner: Mutex::new(Inner { live, changed: now, window: now, bytes: 0 }) }
    }
}

impl Board {
    /// Move to a state, with what went wrong or nothing.
    pub fn set(&self, state: DestinationState, error: Option<String>) {
        let mut b = self.inner.lock();
        if b.live.state != state {
            b.live.state = state;
            b.changed = Instant::now();
        }
        if state != DestinationState::Live {
            b.live.kbps = 0;
        }
        b.live.error = error;
    }

    pub fn reconnected(&self) {
        self.inner.lock().live.reconnects += 1;
    }

    /// Count bytes packaged. Once a second the rate is worked out again.
    pub fn packaged(&self, n: usize) {
        let mut b = self.inner.lock();
        b.bytes += n as u64;
        let took = b.window.elapsed();
        if took >= Duration::from_secs(1) {
            b.live.kbps = (b.bytes * 8 / took.as_millis().max(1) as u64) as u32;
            (b.bytes, b.window) = (0, Instant::now());
        }
    }

    pub fn state(&self) -> DestinationState {
        self.inner.lock().live.state
    }

    pub fn read(&self) -> DestinationLive {
        let b = self.inner.lock();
        DestinationLive { since_ms: b.changed.elapsed().as_millis() as u64, ..b.live.clone() }
    }
}
