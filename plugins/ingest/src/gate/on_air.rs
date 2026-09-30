//! Every publisher let in on a channel, with the key and the protocol that
//! let it in and a way to cut it off.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::channels::{Protocol, Table};
use crate::rtmp::Kick;

type Row = (u64, String, String, Protocol, Kick);

#[derive(Default)]
pub struct OnAir {
    next: AtomicU64,
    list: Mutex<Vec<Row>>,
}

impl OnAir {
    pub fn add(&self, channel: &str, key: &str, via: Protocol, kick: Kick) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        self.lock().push((id, channel.to_string(), key.to_string(), via, kick));
        id
    }

    pub fn remove(&self, id: u64) {
        self.lock().retain(|(i, ..)| *i != id);
    }

    /// The kicks of everyone `table` no longer lets in: a key taken back, a
    /// channel switched off or removed, or its protocol switched off.
    pub fn not_admitted_by(&self, table: &Table) -> Vec<Kick> {
        self.lock()
            .iter()
            .filter(|(_, channel, key, via, _)| !table.still_admits(channel, key, *via))
            .map(|(.., kick)| kick.clone())
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Row>> {
        self.list.lock().unwrap_or_else(|e| e.into_inner())
    }
}
