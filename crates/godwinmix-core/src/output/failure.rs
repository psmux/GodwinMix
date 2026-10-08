//! What a destination's bus error means, in words a person can act on.
//!
//! `rtmp2sink` says exactly what went wrong ("Connection refused", "Socket
//! I/O timed out", "Error resolving", "NetStream.Publish.Denied") and the
//! mixer used to log it at debug and retry. The page said "Reconnecting,
//! attempt 3", the platform said "No data", and the person in between had no
//! way to tell a wrong key from a firewall. So the error is kept, read into a
//! reason and a sentence with the next step in it, and put on the status, and
//! the first one after a start or a drop goes out as an alert as well.
//!
//! Nothing here touches a pipeline. It runs on the mixer thread with the
//! message the bus watcher already copied out.

use godwinmix_protocol::output_error::{OutputError, OutputErrorReason};
use godwinmix_protocol::types::{Event, Severity};
use godwinmix_protocol::ErrorAction;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The last failure of one output, and whether anybody has been told.
#[derive(Default)]
pub struct Failure {
    last: Mutex<Option<OutputError>>,
    /// Set when a new attempt starts, so its first error replaces the last
    /// attempt's rather than being judged against it.
    fresh: AtomicBool,
    /// An alert has gone out for this run of failures. Cleared when the
    /// output connects, so a drop later is told once again.
    told: AtomicBool,
    /// It has been live at least once since it was added or changed, which
    /// is the difference between "did not start" and "lost its connection".
    was_live: AtomicBool,
}

impl Failure {
    /// A new attempt to connect is starting.
    pub fn attempt(&self) {
        self.fresh.store(true, Ordering::Relaxed);
    }

    /// One bus error from this output's pipeline. A dying connection posts
    /// several, the sink's own first and then "Internal data stream error"
    /// from everything upstream of it; the specific one is the one kept.
    pub fn note(&self, message: &str, uri: &str) {
        let next = classify(message, uri);
        let mut last = self.last.lock();
        let replace = self.fresh.swap(false, Ordering::Relaxed)
            || last.as_ref().is_none_or(|l| l.reason == OutputErrorReason::Other && next.reason != OutputErrorReason::Other);
        if replace {
            *last = Some(next);
        }
    }

    /// Connected: the error is history.
    pub fn connected(&self) {
        *self.last.lock() = None;
        self.told.store(false, Ordering::Relaxed);
        self.was_live.store(true, Ordering::Relaxed);
    }

    pub fn current(&self) -> Option<OutputError> {
        self.last.lock().clone()
    }

    /// The alert for this run of failures, once, or nothing.
    pub fn alert(&self, id: &str) -> Option<Event> {
        let error = self.current()?;
        if self.told.swap(true, Ordering::Relaxed) {
            return None;
        }
        let (severity, what) = if self.was_live.load(Ordering::Relaxed) {
            (Severity::Warning, "lost its connection")
        } else {
            (Severity::Error, "did not start")
        };
        Some(Event::Alert {
            severity,
            message: format!("{id} {what}. {} It keeps trying by itself.", error.message),
            action: Some(Box::new(ErrorAction::open_panel("Show Outputs", "core/outputs"))),
        })
    }
}

/// What the overflow watchdog notes for a destination that filled its buffer
/// without ever going live. `rtmp2sink` posts nothing at all when a server
/// answers a publish with `_error` and no transaction, which is how the
/// mixer's own ingest turns a wrong key away: it waits for an answer that
/// never comes, and the buffer filling is the only sign.
pub const STALLED: &str = "the server took the connection and never took the stream";

/// A bus error's text as a reason and a sentence for a person.
pub fn classify(message: &str, uri: &str) -> OutputError {
    let lower = message.to_lowercase();
    let has = |words: &[&str]| words.iter().any(|w| lower.contains(w));
    let reason = if lower == STALLED {
        OutputErrorReason::Stalled
    } else if has(&["refused"]) {
        OutputErrorReason::Refused
    } else if has(&["timed out", "timeout"]) {
        OutputErrorReason::TimedOut
    } else if has(&["resolv", "no such host", "not known", "nodename nor servname", "name or service"]) {
        OutputErrorReason::NotFound
    } else if has(&["unreachable", "no route", "network is down"]) {
        OutputErrorReason::Unreachable
    } else if has(&["denied", "not authorized", "unauthorized", "rejected", "badname", "forbidden", "cmd failed"]) {
        OutputErrorReason::Rejected
    } else if has(&["short read", "closed", "reset", "broken pipe", "end of file", "eof"]) {
        OutputErrorReason::Closed
    } else {
        OutputErrorReason::Other
    };
    let detail = scrub(message, uri);
    let message = sentence(reason, uri, &detail);
    OutputError { reason, message, detail }
}

/// What to say for each reason. `host` is the server and its port, never the
/// path, which is where every platform keeps the key.
fn sentence(reason: OutputErrorReason, uri: &str, detail: &str) -> String {
    let (host, name, port) = host_of(uri);
    match reason {
        OutputErrorReason::Refused => format!(
            "{host} refused the connection: nothing there is taking streams. Check the server address and port, and that the server is running."
        ),
        OutputErrorReason::TimedOut => format!(
            "{host} did not answer. Check the server address, and that a firewall or VPN is not blocking outgoing connections to port {port}."
        ),
        OutputErrorReason::NotFound => {
            format!("There is no server called {name}. Check the server address for a typing mistake.")
        }
        OutputErrorReason::Unreachable => {
            format!("{host} cannot be reached from this machine. Check the server address and the internet connection.")
        }
        OutputErrorReason::Rejected => format!(
            "{host} turned the stream away, which usually means the stream key is wrong or has expired. Copy the key again from the platform and paste it under Edit, Replace key."
        ),
        OutputErrorReason::Closed => format!(
            "{host} hung up as the stream started. A platform does this with a stream key it does not recognise: copy the key again, and check the live stream is set up on the platform's side."
        ),
        OutputErrorReason::Stalled => format!(
            "{host} answered but has not taken any of the stream. A server does this with a stream key it does not know, or with no live stream set up to receive it: copy the key again, and check the platform's side."
        ),
        OutputErrorReason::Other if detail.is_empty() => format!("The connection to {host} failed."),
        OutputErrorReason::Other => format!("The connection to {host} failed: {detail}"),
    }
}

/// `host:port`, the host alone, and the port, out of an address. The port is
/// the scheme's own when the address does not name one.
fn host_of(uri: &str) -> (String, String, u16) {
    let (scheme, rest) = uri.split_once("://").unwrap_or(("", uri));
    let hostport = rest.split(['/', '?']).next().unwrap_or("");
    let hostport = hostport.rsplit('@').next().unwrap_or(hostport);
    let default = match scheme.to_ascii_lowercase().as_str() {
        "rtmps" => 443,
        _ => 1935,
    };
    match hostport.rsplit_once(':') {
        Some((h, p)) if p.parse::<u16>().is_ok() => (hostport.to_string(), h.to_string(), p.parse().unwrap_or(default)),
        _ => (hostport.to_string(), hostport.to_string(), default),
    }
}

/// The sink's message with every piece of the address's path cut out. A
/// server's refusal can quote the stream name back, and on every platform
/// the stream name is the key.
pub fn scrub(message: &str, uri: &str) -> String {
    let rest = uri.split_once("://").map_or(uri, |(_, r)| r);
    let path = rest.split_once('/').map_or("", |(_, p)| p);
    let mut out = message.trim().to_string();
    let mut pieces: Vec<&str> = std::iter::once(path)
        .chain(path.split(['/', '?', '&', '=']))
        .filter(|p| p.len() >= 6)
        .collect();
    // Longest first, so the whole path goes before a part of it would leave
    // the rest of the key standing.
    pieces.sort_by_key(|p| std::cmp::Reverse(p.len()));
    for p in pieces {
        out = out.replace(p, "…");
    }
    out
}

#[cfg(test)]
#[path = "failure_tests.rs"]
mod tests;
