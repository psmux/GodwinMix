//! What a destination reports, written by its sender thread and read by
//! anyone. The lock is held for a field assignment and never across I/O.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use godwinmix_protocol::destination::{DestinationLive, DestinationState};

pub struct Board {
    inner: Mutex<Inner>,
}

struct Inner {
    live: DestinationLive,
    changed: Instant,
    bytes: u64,
    window_start: Instant,
    window_bytes: u64,
}

impl Board {
    pub fn new() -> Board {
        let now = Instant::now();
        Board {
            inner: Mutex::new(Inner {
                live: DestinationLive::default(),
                changed: now,
                bytes: 0,
                window_start: now,
                window_bytes: 0,
            }),
        }
    }

    fn with<T>(&self, f: impl FnOnce(&mut Inner) -> T) -> T {
        f(&mut self.inner.lock().unwrap_or_else(|e| e.into_inner()))
    }

    /// Move to a state. The clock restarts only when it is a new one.
    pub fn state(&self, state: DestinationState) {
        self.with(|b| {
            if b.live.state != state {
                b.live.state = state;
                b.changed = Instant::now();
            }
            if state != DestinationState::Live {
                b.live.kbps = 0;
            }
        })
    }

    pub fn error(&self, error: Option<String>) {
        self.with(|b| b.live.error = error)
    }

    pub fn reconnected(&self) {
        self.with(|b| b.live.reconnects += 1)
    }

    /// Count bytes sent. Once a second the rate is worked out again from
    /// the last window, which is what `kbps` reports.
    pub fn sent(&self, n: usize) {
        self.with(|b| {
            b.bytes += n as u64;
            b.window_bytes += n as u64;
            let elapsed = b.window_start.elapsed();
            if elapsed >= Duration::from_secs(1) {
                b.live.kbps = (b.window_bytes * 8 / elapsed.as_millis().max(1) as u64) as u32;
                b.window_start = Instant::now();
                b.window_bytes = 0;
            }
        })
    }

    /// A copy for a reader, with `since_ms` worked out now.
    pub fn read(&self) -> (DestinationLive, u64) {
        self.with(|b| {
            let mut live = b.live.clone();
            live.since_ms = b.changed.elapsed().as_millis() as u64;
            (live, b.bytes)
        })
    }
}
