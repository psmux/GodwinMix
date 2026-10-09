//! `channel.stream.state` and `channel.destination.state`: the hooks a
//! channel fires, beside `output.state` for a mixer's own outputs.
//!
//! The channels do not know how a hook is reached. Whoever owns the hooks
//! (the core's control plane, or the station) hands them a [`Hook`], and
//! until one is handed over nothing is fired. A payload is built only when
//! some hook is configured for the event.

use std::sync::Arc;

use serde_json::{json, Value};

use super::Channels;

/// Where a channel's hooks go.
pub trait Hook: Send + Sync {
    /// Whether any hook is configured for `event`.
    fn wants(&self, event: &str) -> bool;
    /// Fire `event` with `payload`, and do not wait.
    fn fire(&self, event: &'static str, payload: Value);
}

pub use godwinmix_core::hooks::name::{CHANNEL_DESTINATION_STATE as DESTINATION_STATE, CHANNEL_STREAM_STATE as STREAM_STATE};

impl Channels {
    /// Fire the channel hooks through `hook` from now on.
    pub fn set_hooks(&self, hook: Arc<dyn Hook>) {
        let _ = self.hooks.set(hook);
    }

    fn hook(&self, event: &'static str, payload: impl FnOnce() -> Value) {
        if let Some(h) = self.hooks.get().filter(|h| h.wants(event)) {
            h.fire(event, payload());
        }
    }

    /// A stream went live or idle.
    pub(super) fn hook_stream(&self, channel: &str, stream: &str, state: &str, live: Option<&super::Live>) {
        self.hook(STREAM_STATE, || {
            json!({
                "channel": channel,
                "stream": stream,
                "state": state,
                "from": live.map(|l| l.from.clone()),
                "protocol": live.and_then(|l| l.protocol.clone()),
            })
        });
    }

    /// A destination's state, error or reconnect count moved.
    pub(super) fn hook_destination(&self, channel: &str, destination: &str, live: &godwinmix_protocol::destination::DestinationLive) {
        self.hook(DESTINATION_STATE, || {
            json!({
                "channel": channel,
                "destination": destination,
                "state": live.state,
                "reconnects": live.reconnects,
                "error": live.error,
            })
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    /// Keeps what was fired, for one event name.
    pub struct Recorder {
        pub event: &'static str,
        pub fired: Mutex<Vec<(String, Value)>>,
    }

    impl Hook for Recorder {
        fn wants(&self, event: &str) -> bool {
            event == self.event
        }
        fn fire(&self, event: &'static str, payload: Value) {
            self.fired.lock().push((event.to_string(), payload));
        }
    }

    #[tokio::test]
    async fn a_hook_is_fired_only_for_the_event_it_is_configured_for() {
        let channels = super::super::default::tests::open_bare("hooks");
        let rec = Arc::new(Recorder { event: DESTINATION_STATE, fired: Mutex::default() });
        channels.set_hooks(rec.clone());
        channels.hook_stream("church", "main", "live", None);
        assert!(rec.fired.lock().is_empty(), "nobody asked for channel.stream.state");
        let live = godwinmix_protocol::destination::DestinationLive {
            state: godwinmix_protocol::destination::DestinationState::Reconnecting,
            since_ms: 0,
            kbps: 0,
            reconnects: 2,
            error: Some("the far end closed the connection".into()),
        };
        channels.hook_destination("church", "youtube", &live);
        let fired = rec.fired.lock();
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].0, DESTINATION_STATE);
        assert_eq!(fired[0].1["state"], "reconnecting");
        assert_eq!(fired[0].1["reconnects"], 2);
        assert_eq!(fired[0].1["error"], "the far end closed the connection");
    }

    #[tokio::test]
    async fn a_stream_going_live_and_leaving_fires_channel_stream_state_once_each() {
        let channels = super::super::default::tests::open_bare("hooks-stream");
        let req = serde_json::from_value(json!({"name": "Church", "auto_source": false})).unwrap();
        channels.add(req).unwrap();
        let rec = Arc::new(Recorder { event: STREAM_STATE, fired: Mutex::default() });
        channels.set_hooks(rec.clone());
        let live = json!({"channel": "church", "app": "church", "stream": "main", "state": "live", "from": "10.0.0.9:5000", "protocol": "rtmp"});
        channels.went_live(&live);
        // A second report about the same session is news of its numbers, not
        // of its state.
        channels.went_live(&live);
        channels.went_idle(&json!({"channel": "church", "app": "church", "stream": "main", "state": "idle"}));
        let fired = rec.fired.lock();
        let states: Vec<&str> = fired.iter().map(|(_, p)| p["state"].as_str().unwrap()).collect();
        assert_eq!(states, ["live", "idle"]);
        assert_eq!(fired[0].1["from"], "10.0.0.9:5000");
        assert_eq!((fired[0].1["channel"].as_str(), fired[0].1["stream"].as_str()), (Some("church"), Some("main")));
    }
}
