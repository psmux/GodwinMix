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
mod table;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use godwinmix_protocol::destination::DestinationState;
use godwinmix_sdk::plugin::Reporter;
use serde_json::{json, Value};

use crate::hub::Hub;
use crate::transcode::{StreamSpec, Transcoders};
use runner::Runner;
pub use table::{destination, wanted, Feed, Wanted};

/// The running destinations.
pub struct Sends {
    hub: Hub,
    /// Streams converted for the destinations that asked for a rendition.
    transcoders: Arc<Transcoders>,
    reporter: Option<Reporter>,
    running: Arc<Mutex<Vec<Arc<Runner>>>>,
    watching: Arc<AtomicBool>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl Sends {
    pub fn new(hub: Hub, reporter: Option<Reporter>) -> Sends {
        let transcoders = Arc::new(Transcoders::new(hub.clone()));
        Sends { hub, transcoders, reporter, running: Default::default(), watching: Default::default() }
    }

    /// Make what runs match what is wanted. A destination that changed in
    /// any way is stopped and started again; one that did not is untouched.
    /// The streams `specs` converts are built or changed first, node by
    /// node, so a converting destination has its pair to read.
    pub fn apply(&self, wanted: Vec<Wanted>, specs: Vec<StreamSpec>) {
        self.transcoders.apply(specs, &wanted);
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
                let hub = match w.feed {
                    Feed::Copy => self.hub.clone(),
                    Feed::Rendition { .. } => self.transcoders.renditions(),
                };
                running.push(Runner::start(w, hub));
            }
        }
        let any = !running.is_empty();
        drop(running);
        if any {
            self.watch();
        }
    }

    /// Where converted pairs are published, for a test to read one.
    #[cfg(test)]
    pub fn renditions(&self) -> Hub {
        self.transcoders.renditions()
    }

    /// What each destination is sending now, for the `streams` tool.
    pub fn rates(&self) -> Vec<Value> {
        lock(&self.running)
            .iter()
            .map(|r| {
                let live = live_of(r, &self.transcoders);
                let mut row = json!({"channel": r.wanted.channel, "destination": r.wanted.id, "state": live.state, "kbps": live.kbps});
                if let Some(f) = &live.file {
                    row["file"] = json!(f);
                }
                row
            })
            .collect()
    }

    /// Start the watch thread unless it is running.
    fn watch(&self) {
        if self.watching.swap(true, Ordering::AcqRel) {
            return;
        }
        let (running, watching, reporter) = (self.running.clone(), self.watching.clone(), self.reporter.clone());
        let transcoders = self.transcoders.clone();
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
                    report(r, &mut said, reporter.as_ref(), &transcoders);
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
fn report(r: &Runner, said: &mut Vec<(String, String, Value)>, reporter: Option<&Reporter>, t: &Transcoders) {
    let live = live_of(r, t);
    let key = json!([live.state, live.error, live.reconnects, live.file.as_ref().map(|f| (&f.name, f.open))]);
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

/// What a destination is doing, with a converting destination whose pair
/// cannot be made shown as failed, with why, rather than waiting forever.
fn live_of(r: &Runner, t: &Transcoders) -> godwinmix_protocol::destination::DestinationLive {
    let mut live = r.stats();
    if live.state == DestinationState::Waiting {
        if let Some(why) = t.error(&r.wanted) {
            live.state = DestinationState::Failed;
            live.error = Some(why);
        }
    }
    live
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

#[cfg(test)]
#[path = "sends/record_tests.rs"]
mod record_tests;
