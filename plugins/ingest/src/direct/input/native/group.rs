//! One multicast group (or unicast port) and the shows that take programs
//! from it. The socket thread (`pump`) reads it; each show holds a
//! [`Subscription`] and reads its own program's packets from a queue.
//!
//! The subscriber list is behind a lock the socket thread holds while it
//! hands one batch out, which is copying and `try_send`, never I/O. A show
//! that falls behind fills its own queue and loses packets there, counted
//! in `dropped`; the socket thread never waits for it.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering::Relaxed};
use std::sync::mpsc::{Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};

use gmx_udp::counters::{Counters, SharedCatalog};

/// Batches a show may have waiting: at 32 datagrams a batch, a few seconds
/// of one program even at the full rate of a multiplex.
pub const QUEUE: usize = 512;

/// What a group is: the same key is the same socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Key {
    pub host: String,
    pub port: u16,
    pub source: Option<String>,
    pub interface: String,
}

/// What the socket thread hands a show.
pub enum Item {
    /// Follow this program, whose PMT is on this PID.
    Follow(u16, u16),
    /// The program's own packets, whole, in arrival order.
    Packets(Vec<u8>),
    /// The feed was quiet long enough that its sender may have restarted.
    Resumed,
}

pub struct Sub {
    pub id: u64,
    /// The program asked for, or `None` for the first the PAT lists.
    pub program: Option<u16>,
    pub tx: SyncSender<Item>,
    /// The program and PMT PID last sent as `Follow`.
    pub routed: Option<(u16, u16)>,
    /// This batch's packets, on their way.
    pub buf: Vec<u8>,
    pub dropped: Arc<AtomicU64>,
}

#[derive(Default)]
pub struct Subs {
    pub list: Vec<Sub>,
    /// Someone came or went since the routes were built.
    pub changed: bool,
    /// The socket thread has seen the list empty and is closing.
    pub closed: bool,
}

pub struct Group {
    pub key: Key,
    /// As a person would write it, for messages.
    pub address: String,
    pub counters: Arc<Counters>,
    pub catalog: SharedCatalog,
    pub subs: Mutex<Subs>,
    /// Why the socket is not open, while it is not.
    pub error: Mutex<Option<String>>,
    /// The receive buffer the OS gave, in bytes.
    pub buffer: AtomicUsize,
    next: AtomicU64,
}

impl Group {
    pub fn new(key: Key, address: String) -> Group {
        Group {
            key,
            address,
            counters: Arc::default(),
            catalog: SharedCatalog::default(),
            subs: Mutex::default(),
            error: Mutex::new(None),
            buffer: AtomicUsize::new(0),
            next: AtomicU64::new(1),
        }
    }

    pub fn subs(&self) -> MutexGuard<'_, Subs> {
        self.subs.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Add a show taking `program`, unless the group is closing. Answers
    /// its id, queue and loss counter.
    pub fn add(&self, program: Option<u16>) -> Option<(u64, Receiver<Item>, Arc<AtomicU64>)> {
        let mut s = self.subs();
        if s.closed {
            return None;
        }
        let (tx, rx) = std::sync::mpsc::sync_channel(QUEUE);
        let (id, dropped) = (self.next.fetch_add(1, Relaxed), Arc::new(AtomicU64::new(0)));
        s.list.push(Sub { id, program, tx, routed: None, buf: Vec::new(), dropped: dropped.clone() });
        s.changed = true;
        Some((id, rx, dropped))
    }

    pub fn remove(&self, id: u64) {
        let mut s = self.subs();
        s.list.retain(|x| x.id != id);
        s.changed = true;
    }

    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_error(&self, e: Option<String>) {
        *self.error.lock().unwrap_or_else(|e| e.into_inner()) = e;
    }
}

/// One show's hold on a group. Dropping it leaves the group, and the last
/// one to leave closes the socket.
pub struct Subscription {
    pub group: Arc<Group>,
    pub id: u64,
    pub rx: Receiver<Item>,
    /// Packets that did not fit in this show's queue.
    pub dropped: Arc<AtomicU64>,
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.group.remove(self.id);
    }
}
