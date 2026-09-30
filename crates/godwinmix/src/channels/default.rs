//! The default channel: `live`, made once for a mixer that has the ingest
//! plugin and no channels, so an encoder can be pointed at it from the first
//! start without anybody making one.
//!
//! It is made once. The channels file remembers that it was (`default_made`),
//! so a person who deletes it does not find it back at the next start, and a
//! mixer that already had channels of its own when this arrived gets none.
//! A core with no config file on disk gets none either: it could not
//! remember the deletion, and would make it again at every start.

use std::sync::atomic::Ordering;

use godwinmix_protocol::channel_ingest::{rtmp_only, Rtmps};
use godwinmix_protocol::channels::KeyMode;
use tracing::{info, warn};

use super::store::Record;
use super::Channels;

/// Its id, and the application name in its address.
pub const ID: &str = "live";
const NAME: &str = "Live";
const KEY_LABEL: &str = "Default key";

impl Channels {
    /// Make the default channel if it is due, before the listener is first
    /// handed its table. `ingest` is whether the plugin is installed.
    pub(super) fn make_default(&self, ingest: bool) -> bool {
        if !ingest || self.store.is_none() || self.default_made.load(Ordering::Relaxed) {
            return false;
        }
        {
            let mut records = self.records.lock();
            if !records.is_empty() {
                return false;
            }
            records.push(record());
        }
        if let Err(e) = self.make_key(ID, Some(KEY_LABEL.into())) {
            warn!(error = %e.message, "the default channel's key could not be sealed; no default channel is made");
            self.records.lock().retain(|r| r.id != ID);
            return false;
        }
        self.default_made.store(true, Ordering::Relaxed);
        if let Err(e) = self.persist() {
            warn!(error = %format!("{e:#}"), "the default channel could not be saved; it is served until the next start");
        }
        info!(channel = ID, "made the default channel");
        true
    }

    /// The same, on a running core: after the ingest plugin is installed
    /// from the page, so the Channels tab has its channel at once.
    pub fn ensure_default(&self) {
        if self.make_default(godwinmix_core::plugin::loader::get(super::PLUGIN).is_some()) {
            self.hand_over(true);
            self.announce(ID);
        }
    }
}

fn record() -> Record {
    Record {
        id: ID.into(),
        name: NAME.into(),
        app: ID.into(),
        enabled: true,
        auto_source: true,
        key_mode: KeyMode::default(),
        protocols: rtmp_only(),
        rtmps: Rtmps::default(),
        keys: Vec::new(),
        auto_sources: Vec::new(),
        destinations: Vec::new(),
        extra: Default::default(),
    }
}

#[cfg(test)]
#[path = "default_tests.rs"]
mod tests;
