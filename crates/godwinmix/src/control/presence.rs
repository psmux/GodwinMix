//! Who is connected to `/rpc`, for `presence.list` and `event/presence.changed`.
//!
//! Each connection takes a seat when it opens and gives it back when it
//! closes, which is a map insert and a map remove. Nothing else happens unless
//! somebody asked: the list is built when `presence.list` is called or when a
//! subscriber is told it changed, and a change is announced only while at
//! least one connection is subscribed to `presence.changed`.

use godwinmix_protocol::presence::PresenceList;
use godwinmix_protocol::scope::Token;
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::broadcast;

mod device;
mod seat;

pub use device::device_of;
use seat::Seat;

#[derive(Debug)]
pub struct Presence {
    seats: Mutex<BTreeMap<u64, Seat>>,
    next: AtomicU64,
    changed: broadcast::Sender<()>,
}

impl Presence {
    pub fn new() -> Arc<Presence> {
        Arc::new(Presence {
            seats: Mutex::new(BTreeMap::new()),
            next: AtomicU64::new(1),
            changed: broadcast::channel(16).0,
        })
    }

    /// A name for a connection that did not choose one: `s1`, `s2`, and so
    /// on for the life of the process, so two connections never share one.
    pub fn fresh_name(&self) -> String {
        format!("s{}", self.next.fetch_add(1, Ordering::Relaxed))
    }

    /// Take a seat. Dropping what this returns gives it back.
    pub fn join(self: &Arc<Self>, client_id: &str, token: &Token, token_label: Option<String>, user_agent: Option<&str>) -> Here {
        let key = self.next.fetch_add(1, Ordering::Relaxed);
        self.seats.lock().insert(key, Seat::new(client_id, token, token_label, user_agent));
        self.announce();
        Here { presence: self.clone(), key }
    }

    /// Everybody connected, oldest first, with `you` set on `asking`.
    pub fn list(&self, asking: Option<&str>) -> PresenceList {
        PresenceList { clients: self.seats.lock().values().map(|s| s.listed(asking)).collect() }
    }

    /// What a connection says about itself. False when no connection has
    /// this client id.
    pub fn set(&self, client_id: &str, scene: Option<String>, label: Option<String>) -> bool {
        let mut found = false;
        for seat in self.seats.lock().values_mut().filter(|s| s.client_id == client_id) {
            seat.scene = scene.clone();
            if let Some(label) = &label {
                seat.label = Some(label.trim().to_string()).filter(|l| !l.is_empty());
            }
            found = true;
        }
        if found {
            self.announce();
        }
        found
    }

    /// How to name a client to a person, while it is connected.
    pub fn who(&self, client_id: &str) -> Option<String> {
        self.seats.lock().values().find(|s| s.client_id == client_id)?.name()
    }

    /// Be told whenever the list changes. Holding a receiver is what makes a
    /// change announced at all.
    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    fn announce(&self) {
        if self.changed.receiver_count() > 0 {
            let _ = self.changed.send(());
        }
    }
}

/// A connection's seat, given back when the connection ends.
pub struct Here {
    presence: Arc<Presence>,
    key: u64,
}

impl Drop for Here {
    fn drop(&mut self) {
        self.presence.seats.lock().remove(&self.key);
        self.presence.announce();
    }
}

#[cfg(test)]
mod tests;
