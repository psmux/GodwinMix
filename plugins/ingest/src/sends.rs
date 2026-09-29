//! A channel's destinations, run: one restream per destination that is on,
//! reading its stream from the hub.
//!
//! ```text
//!   configure ──► Sends::apply ──► Runner (one thread each) ──► hub.subscribe ──► restream::start
//!                                     ▲                                               │
//!   event/channel.destination ◄── watch thread ◄──────────── stats ──────────────────┘
//! ```
//!
//! The core hands the destinations over in the channel table, each with its
//! whole address. A destination whose stream is not live waits; `*` is the
//! channel's first live stream, and when that one leaves the runner moves on
//! to the next. A reader of the hub has a bounded queue of its own, and the
//! restreamer another behind it, so nothing a destination does reaches the
//! publisher or the other destinations.
//!
//! The watch thread raises `event/channel.destination` when a destination's
//! state, error or reconnect count moves, and exists only while there is a
//! destination to watch. Bit rates are answered in the `streams` tool, which
//! the core reads when a client asks.

mod runner;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use godwinmix_protocol::destination::DestinationState;
use godwinmix_sdk::plugin::Reporter;
use serde_json::{json, Value};

use crate::hub::Hub;
use runner::Runner;

/// One destination as the core asked for it.
#[derive(Debug, Clone, PartialEq)]
pub struct Wanted {
    pub channel: String,
    pub app: String,
    pub id: String,
    pub platform: String,
    /// The whole address, key and all.
    pub url: String,
    /// A stream name, or `*` for the first live one.
    pub stream: String,
}

/// Every destination in the channel table, skipping any that is not well
/// formed. The core only hands over the ones that are on.
pub fn wanted(params: &Value) -> Vec<Wanted> {
    let channels = params.get("channels").and_then(Value::as_array).cloned().unwrap_or_default();
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let mut out = Vec::new();
    for c in &channels {
        let (channel, app) = (text(c, "id"), text(c, "app"));
        let enabled = c.get("enabled").and_then(Value::as_bool).unwrap_or(true);
        for d in c.get("destinations").and_then(Value::as_array).into_iter().flatten() {
            let w = Wanted {
                channel: channel.clone(),
                app: if app.is_empty() { channel.clone() } else { app.clone() },
                id: text(d, "id"),
                platform: text(d, "platform"),
                url: text(d, "url"),
                stream: Some(text(d, "stream")).filter(|s| !s.is_empty()).unwrap_or_else(|| "*".into()),
            };
            let on = d.get("enabled").and_then(Value::as_bool).unwrap_or(true);
            if enabled && on && !w.channel.is_empty() && !w.id.is_empty() && !w.url.is_empty() {
                out.push(w);
            }
        }
    }
    out
}

/// The running destinations.
pub struct Sends {
    hub: Hub,
    reporter: Option<Reporter>,
    running: Arc<Mutex<Vec<Arc<Runner>>>>,
    watching: Arc<AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Sends {
    pub fn new(hub: Hub, reporter: Option<Reporter>) -> Sends {
        Sends { hub, reporter, running: Default::default(), watching: Default::default() }
    }

    /// Make what runs match what is wanted. A destination that changed in
    /// any way is stopped and started again; one that did not is untouched.
    pub fn apply(&self, wanted: Vec<Wanted>) {
        let mut running = lock(&self.running);
        running.retain(|r| {
            let keep = wanted.contains(&r.wanted);
            if !keep {
                r.stop();
            }
            keep
        });
        for w in wanted {
            if !running.iter().any(|r| r.wanted == w) {
                if let Some(r) = &self.reporter {
                    r.info(format!("sending {}/{} to destination {}", w.app, w.stream, w.id));
                }
                running.push(Runner::start(w, self.hub.clone()));
            }
        }
        let any = !running.is_empty();
        drop(running);
        if any {
            self.watch();
        }
    }

    /// What each destination is sending now, for the `streams` tool.
    pub fn rates(&self) -> Vec<Value> {
        lock(&self.running)
            .iter()
            .map(|r| {
                let live = r.stats();
                json!({"channel": r.wanted.channel, "destination": r.wanted.id, "state": live.state, "kbps": live.kbps})
            })
            .collect()
    }

    /// Start the watch thread unless it is running.
    fn watch(&self) {
        if self.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let (running, watching, reporter) = (self.running.clone(), self.watching.clone(), self.reporter.clone());
        let started = std::thread::Builder::new().name("gmx-sends-watch".into()).spawn(move || {
            let mut said: Vec<(String, String, Value)> = Vec::new();
            loop {
                let now: Vec<Arc<Runner>> = lock(&running).clone();
                if now.is_empty() {
                    watching.store(false, Ordering::Release);
                    return;
                }
                said.retain(|(c, d, _)| now.iter().any(|r| &r.wanted.channel == c && &r.wanted.id == d));
                for r in &now {
                    report(r, &mut said, reporter.as_ref());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        });
        if started.is_err() {
            self.watching.store(false, Ordering::Release);
        }
    }
}

/// Raise `event/channel.destination` for one runner if what it says moved.
fn report(r: &Runner, said: &mut Vec<(String, String, Value)>, reporter: Option<&Reporter>) {
    let live = r.stats();
    let key = json!([live.state, live.error, live.reconnects]);
    let (channel, id) = (&r.wanted.channel, &r.wanted.id);
    match said.iter_mut().find(|(c, d, _)| c == channel && d == id) {
        Some((.., last)) if *last == key => return,
        Some((.., last)) => *last = key,
        None => said.push((channel.clone(), id.clone(), key)),
    }
    if let Some(rep) = reporter {
        if live.state == DestinationState::Reconnecting || live.state == DestinationState::Failed {
            if let Some(e) = &live.error {
                rep.warn(format!("destination {id} of {channel}: {e}"));
            }
        }
        let mut params = serde_json::to_value(&live).unwrap_or_else(|_| json!({}));
        params["channel"] = json!(channel);
        params["destination"] = json!(id);
        rep.event("channel.destination", params);
    }
}

impl Drop for Sends {
    fn drop(&mut self) {
        for r in lock(&self.running).drain(..) {
            r.stop();
        }
    }
}

#[cfg(test)]
#[path = "sends/tests.rs"]
mod tests;
