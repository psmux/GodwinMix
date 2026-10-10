//! Telling the core when the mount comes and goes.

use std::sync::atomic::Ordering;

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;

use super::State;

/// The sender's state, and the core told when the mount comes and goes, so
/// the output reads live only while the mount is taking the sound.
pub(super) struct Announced<'a> {
    pub state: &'a State,
    pub told: Option<Reporter>,
}

impl Announced<'_> {
    pub fn up(&self) {
        self.state.connected.store(true, Ordering::Relaxed);
        *self.state.last_error.lock().unwrap_or_else(|e| e.into_inner()) = None;
        if let Some(r) = &self.told {
            r.health_changed(Health::ok());
        }
    }

    pub fn down(&self, why: String) {
        let was = self.state.connected.swap(false, Ordering::Relaxed);
        if let (true, Some(r)) = (was, &self.told) {
            r.health_changed(Health::degraded(format!("{why} Trying again.")));
        }
        *self.state.last_error.lock().unwrap_or_else(|e| e.into_inner()) = Some(why);
    }
}

impl std::ops::Deref for Announced<'_> {
    type Target = State;
    fn deref(&self) -> &State {
        self.state
    }
}
