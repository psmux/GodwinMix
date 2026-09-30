//! A granted share. Dropping it gives the share back.

use crate::governor::Inner;
use godwinmix_protocol::rendition::Cost;
use std::sync::Weak;

/// Held by whatever was started, for as long as it runs. Send and Sync, so
/// it can live in an output's state on any thread.
#[derive(Debug)]
pub struct Ticket {
    id: u64,
    cost: Cost,
    preset: Option<String>,
    /// The number is the station's, and the share is in its book.
    remote: bool,
    // Weak, so a ticket outliving its governor (a test, a shutdown) is
    // harmless: there is no book left to give back to.
    inner: Weak<Inner>,
}

impl Ticket {
    pub(crate) fn new(id: u64, cost: Cost, preset: Option<String>, inner: Weak<Inner>) -> Ticket {
        Ticket { id, cost, preset, remote: false, inner }
    }

    /// A ticket the station granted: dropping it tells the station.
    pub(crate) fn remote(id: u64, cost: Cost, preset: Option<String>, inner: Weak<Inner>) -> Ticket {
        Ticket { id, cost, preset, remote: true, inner }
    }

    /// The id a [`crate::ShedStep`] names.
    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn cost(&self) -> Cost {
        self.cost
    }

    /// The speed preset admission chose, when it chose one.
    pub fn preset(&self) -> Option<&str> {
        self.preset.as_deref()
    }

    /// Record that the work now runs on a faster preset at `cost`, after a
    /// shed step said so and the caller made the change.
    pub fn lower(&mut self, preset: &str, cost: Cost) {
        self.preset = Some(preset.to_string());
        self.cost = cost;
        if let Some(inner) = self.inner.upgrade() {
            if let Some(h) = inner.book.lock().held.get_mut(&self.id) {
                h.cost = cost;
                if let Some(e) = h.encode.as_mut() {
                    e.preset = Some(preset.to_string());
                }
            }
        }
    }
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            match self.remote {
                true => inner.release_remote(self.id),
                false => inner.release(self.id),
            }
        }
    }
}
