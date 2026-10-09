//! What the listener says about each destination it is sending to.
//!
//! The listener raises `event/channel.destination` when a destination's
//! state, error or reconnect count moves, and answers its bit rate in the
//! `streams` tool, which is read when a client asks. Nothing here runs on a
//! timer.

use std::time::{Duration, Instant};

use godwinmix_protocol::destination::{DestinationLive, DestinationState, StoredDestination};
use serde_json::Value;

use super::Channels;

/// The last report about one destination.
pub struct Sending {
    channel: String,
    id: String,
    live: DestinationLive,
    /// When `live.state` began.
    since: Instant,
}

impl Sending {
    fn is(&self, channel: &str, id: &str) -> bool {
        self.channel == channel && self.id == id
    }
}

impl Channels {
    /// A destination's state for a view. One that is off is off, whatever
    /// was last heard; one that is on and not yet heard of is waiting.
    pub(super) fn sending_view(&self, channel: &str, id: &str, enabled: bool) -> DestinationLive {
        if !enabled {
            return DestinationLive::default();
        }
        let sending = self.sending.lock();
        match sending.iter().find(|s| s.is(channel, id)) {
            Some(s) => DestinationLive { since_ms: s.since.elapsed().as_millis() as u64, ..s.live.clone() },
            None => DestinationLive { state: DestinationState::Waiting, ..Default::default() },
        }
    }

    /// `event/channel.destination` from the listener. Announces the channel
    /// when the state, the error or the reconnect count moved.
    pub(super) fn destination_report(&self, v: &Value) {
        let channel = v["channel"].as_str().unwrap_or_default().to_string();
        let id = v["destination"].as_str().unwrap_or_default().to_string();
        let known = self
            .records
            .lock()
            .iter()
            .any(|r| r.id == channel && r.destinations.iter().any(|d| d.id == id && d.enabled));
        let Ok(live) = serde_json::from_value::<DestinationLive>(v.clone()) else { return };
        if !known {
            return;
        }
        let since = Instant::now().checked_sub(Duration::from_millis(live.since_ms)).unwrap_or_else(Instant::now);
        let moved = {
            let mut sending = self.sending.lock();
            match sending.iter_mut().find(|s| s.is(&channel, &id)) {
                Some(s) => {
                    let moved = s.live.state != live.state
                        || s.live.error != live.error
                        || s.live.reconnects != live.reconnects;
                    if s.live.state != live.state {
                        s.since = since;
                    }
                    s.live = live.clone();
                    moved
                }
                None => {
                    sending.push(Sending { channel: channel.clone(), id: id.clone(), live: live.clone(), since });
                    true
                }
            }
        };
        if moved {
            self.hook_destination(&channel, &id, &live);
            self.announce(&channel);
        }
    }

    /// Bit rates, and a recording's file, from a `streams` answer's
    /// `destinations` rows.
    pub(super) fn destination_rates(&self, rows: &[Value]) {
        let mut sending = self.sending.lock();
        for row in rows {
            let (channel, id) = (row["channel"].as_str().unwrap_or(""), row["destination"].as_str().unwrap_or(""));
            if let Some(s) = sending.iter_mut().find(|s| s.is(channel, id)) {
                s.live.kbps = row["kbps"].as_u64().unwrap_or(0) as u32;
                // A recording's size and length move with every read.
                if let Some(file) = row.get("file").and_then(|f| serde_json::from_value(f.clone()).ok()) {
                    s.live.file = Some(file);
                }
            }
        }
    }

    /// After an edit: a destination that went, or that now sends somewhere
    /// else or something else, starts over from waiting.
    pub(super) fn forget_sending(&self, channel: &str, before: &[StoredDestination], after: &[StoredDestination]) {
        let same = |id: &str| {
            let b = before.iter().find(|d| d.id == id);
            let a = after.iter().find(|d| d.id == id);
            matches!((b, a), (Some(b), Some(a)) if b.url() == a.url() && b.stream == a.stream && b.enabled == a.enabled && b.rendition == a.rendition)
        };
        self.sending.lock().retain(|s| s.channel != channel || same(&s.id));
    }

    /// A channel that went takes its reports with it.
    pub(super) fn forget_channel_sending(&self, channel: &str) {
        self.sending.lock().retain(|s| s.channel != channel);
    }
}
