//! Channels that pull: the core's table names an address for a channel, and
//! this module takes the stream from it and publishes it into the channel as
//! `<app>/main`, as if an encoder had, so its destinations, its picture and
//! its numbers all work the same.
//!
//! ```text
//!   table (configure) ──► Pulls::apply ──► one thread per channel ──► direct input ──► ToChannel ──► hub "<app>/main"
//!                                                                         │
//!                  event/channel.pull, event/channel.stream ◄─────────────┘ (once a second, the input's thread)
//! ```
//!
//! The input is the one a direct show uses (`direct/input`): GStreamer, a
//! pipeline of its own, nothing decoded, and a retry for ever with a wait
//! that doubles up to ten seconds. Its bus is read on its own thread, and the
//! sink only hands tags to the hub, which never makes a pusher wait.

mod sink;

use std::sync::{Arc, Mutex, MutexGuard};

use serde_json::{json, Value};

use crate::direct::input::{self, Context, InputSpec};
use crate::direct::StopSignal;
use crate::gate::ChannelGate;
use sink::ToChannel;

/// The name a pulled stream has on its channel.
pub const STREAM: &str = "main";

/// One channel's pull, as the table asks for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Want {
    pub channel: String,
    pub app: String,
    /// The whole address, key and all.
    pub url: String,
}

/// Every channel in the table with a pull address. The core leaves the
/// address out of a channel that is off.
pub fn wanted(params: &Value) -> Vec<Want> {
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let channels = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
    channels
        .iter()
        .filter_map(|c| {
            let url = text(c, "pull");
            let channel = text(c, "id");
            let app = Some(text(c, "app")).filter(|a| !a.is_empty()).unwrap_or_else(|| channel.clone());
            (!url.is_empty() && !channel.is_empty()).then_some(Want { channel, app, url })
        })
        .collect()
}

struct Running {
    want: Want,
    stop: StopSignal,
}

/// The pulls running now.
pub struct Pulls {
    gate: Arc<ChannelGate>,
    running: Mutex<Vec<Running>>,
}

impl Pulls {
    pub fn new(gate: Arc<ChannelGate>) -> Pulls {
        Pulls { gate, running: Mutex::new(Vec::new()) }
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Running>> {
        self.running.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Make what runs match what is wanted. A pull whose address or channel
    /// changed is stopped and started again; one that did not is untouched.
    pub fn apply(&self, wanted: Vec<Want>) {
        let mut running = self.lock();
        running.retain(|r| {
            let keep = wanted.contains(&r.want);
            if !keep {
                r.stop.stop();
            }
            keep
        });
        for w in wanted {
            if !running.iter().any(|r| r.want == w) {
                if let Some(r) = &self.gate.reporter {
                    r.info(format!("channel {} pulls {}", w.channel, godwinmix_protocol::destination::uri_host(&w.url)));
                }
                running.push(self.start(w));
            }
        }
    }

    fn start(&self, want: Want) -> Running {
        let stop = StopSignal::default();
        let sink = ToChannel::new(self.gate.clone(), &want);
        let ctx = Context { hub: Some(self.gate.hub.clone()) };
        let (halt, url) = (stop.clone(), want.url.clone());
        let spawned = std::thread::Builder::new().name(format!("gmx-pull-{}", want.channel)).spawn(move || {
            match input::open(&InputSpec::new(&url), &ctx) {
                Ok(input) => input.run(Box::new(sink), halt),
                Err(e) => sink.refused(&e.message),
            }
        });
        if spawned.is_err() {
            if let Some(r) = &self.gate.reporter {
                let why = "no thread to pull with; it is tried again when the channel next changes";
                r.event("channel.pull", json!({"channel": want.channel, "state": "retrying", "error": why}));
            }
        }
        Running { want, stop }
    }
}

impl Drop for Pulls {
    fn drop(&mut self) {
        for r in self.lock().iter() {
            r.stop.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_channel_with_an_address_pulls() {
        let params = json!({"channels": [
            {"id": "relay", "app": "Relay", "pull": "rtmp://a/b/c"},
            {"id": "hall", "app": "hall"},
            {"id": "bare", "pull": "srt://x:1"},
        ]});
        let w = wanted(&params);
        assert_eq!(w.len(), 2);
        assert_eq!((w[0].app.as_str(), w[0].url.as_str()), ("Relay", "rtmp://a/b/c"));
        assert_eq!(w[1].app, "bare", "the id when the app is left out");
    }
}
