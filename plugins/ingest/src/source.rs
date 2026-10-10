//! `ingest/rtmp`: the mixer waits, the publisher dials in.
//!
//! This is the other direction from the core's built in `rtmp/source`, which
//! dials out to somebody else's RTMP server. Here the mixer is the server. It
//! is how a phone, an OBS on the other laptop, or a hardware encoder in the
//! rack reaches a church or a small studio, and today that needs a separate
//! mediamtx running beside the mixer.
//!
//! Two ways to get the bytes:
//!
//! * with no `relay` set, this source owns the listening port itself. One
//!   source, one port, one publisher. `src/source/listen.rs`.
//! * with `relay` and `stream` set, the bytes come from the channel server
//!   (`ingest/discover`), which holds one port for every channel and hands a
//!   stream to whoever asks for it by name. `src/source/relayed.rs`.
//!
//! Either way what leaves on stdout is Matroska, remuxed from FLV with nothing
//! decoded; `src/remux.rs` says why.

mod listen;
mod relayed;

use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::Health;
use serde_json::Value;

use crate::remux::{Out, Remux};
use crate::rtmp::Server;

/// The settings of `ingest/rtmp`.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub port: u16,
    pub app: String,
    pub stream_key: String,
    /// `host:port` of the channel server. Empty means own the port.
    pub relay: String,
    /// `<app>/<stream>` to read from the channel server.
    pub stream: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            bind: "0.0.0.0".into(),
            // 1935 is the port every encoder offers by default, so a publisher
            // that types nothing but the host still arrives.
            port: 1935,
            // Empty accepts whatever the publisher asks for, which is what
            // "appears as a live source with no configuration" requires.
            app: String::new(),
            stream_key: String::new(),
            relay: String::new(),
            stream: String::new(),
        }
    }
}

fn string(params: &Value, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

impl Settings {
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        Settings {
            bind: string(params, "bind").unwrap_or(d.bind),
            port: params
                .get("port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(d.port),
            app: string(params, "app").unwrap_or(d.app),
            stream_key: string(params, "stream_key").unwrap_or(d.stream_key),
            relay: string(params, "relay").unwrap_or(d.relay),
            stream: string(params, "stream").unwrap_or(d.stream),
        }
    }

    pub fn problem(&self) -> Option<String> {
        if !self.relay.is_empty() && !self.relay.contains(':') {
            return Some(format!(
                "relay is '{}', which is not a host:port. It is filled in by \
                 ingest/discover; leave it empty and this source listens on its own port.",
                self.relay
            ));
        }
        if !self.relay.is_empty() && !self.stream.contains('/') {
            return Some(format!(
                "relay is set but stream is '{}', not <channel>/<stream>. Both are filled in \
                 together when a channel's stream becomes a source; clear relay and this \
                 source listens on its own port instead.",
                self.stream
            ));
        }
        if self.relay.is_empty() && self.bind.is_empty() {
            return Some(
                "bind is empty. Write 0.0.0.0 to listen on every interface, or the address \
                 of the one to listen on."
                    .into(),
            );
        }
        None
    }

    /// The address to give a publisher, for a log line and for `stats`.
    pub fn publish_url(&self, actual_port: u16) -> String {
        let app = if self.app.is_empty() { "live" } else { &self.app };
        let key = if self.stream_key.is_empty() { "<any key>" } else { &self.stream_key };
        format!("rtmp://<this machine>:{actual_port}/{app}/{key}")
    }
}

/// A running `ingest/rtmp`: a listener or a relay reader, and what it has seen.
pub struct Ingest {
    state: Arc<State>,
    /// Held so the listener lives as long as the source does.
    _server: Option<Server>,
    stop: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}

/// What the connection threads write and `health` reads.
pub(crate) struct State {
    /// Who is publishing, or none.
    publisher: Mutex<Option<String>>,
    bytes: AtomicU64,
    /// The port actually bound, which is not known until the listener is up.
    port: AtomicU16,
    /// The address to tell a publisher to use, filled in with the real port.
    where_from: Mutex<String>,
    /// Set when the remuxer has failed. A source whose pipe is broken is not
    /// carrying a picture whatever the publisher thinks.
    broken: AtomicBool,
}

impl State {
    fn new() -> State {
        State {
            publisher: Mutex::new(None),
            bytes: AtomicU64::new(0),
            port: AtomicU16::new(0),
            where_from: Mutex::new(String::new()),
            broken: AtomicBool::new(false),
        }
    }

    fn address(&self) -> String {
        self.where_from.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_address(&self, address: String) {
        *self.where_from.lock().unwrap_or_else(|e| e.into_inner()) = address;
    }

    fn set_publisher(&self, who: Option<String>) {
        *self.publisher.lock().unwrap_or_else(|e| e.into_inner()) = who;
    }

    /// Count bytes that reached the remuxer, and notice when it has failed.
    fn wrote(&self, n: usize, remux: &Remux) {
        self.bytes.fetch_add(n as u64, Ordering::Relaxed);
        if remux.broken() {
            self.broken.store(true, Ordering::Relaxed);
        }
    }
}

impl Ingest {
    /// Start listening, or start reading from the channel server.
    pub fn start(settings: &Settings, reporter: Option<Reporter>, out: Out) -> Result<Ingest, String> {
        // Only a real source process ends itself when its stream does; a
        // test writing to a file must not take the test runner with it.
        let exit_at_end = matches!(out, Out::Stdout);
        let remux = Remux::open(out, reporter.clone())?;
        let state = Arc::new(State::new());
        let stop = Arc::new(AtomicBool::new(false));
        if settings.relay.is_empty() {
            let server = listen::start(settings, reporter, remux, state.clone(), ender(exit_at_end))?;
            return Ok(Ingest { state, _server: Some(server), stop, reader: None });
        }
        let reader = relayed::start(settings, reporter, remux, state.clone(), stop.clone(), exit_at_end)?;
        Ok(Ingest { state, _server: None, stop, reader: Some(reader) })
    }

    /// The port actually listening, or 0 when reading from a relay.
    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.state.port.load(Ordering::Relaxed)
    }

    pub fn health(&self) -> Health {
        if self.state.broken.load(Ordering::Relaxed) {
            return Health::failing(
                "the stream could not be remuxed for the core. The publisher is sending \
                 something this build cannot parse: check its video codec is H.264 and \
                 its audio AAC, which is what RTMP carries.",
            );
        }
        let publisher = self.state.publisher.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match publisher {
            Some(who) => {
                let mut health = Health::ok();
                health.detail = Some(format!("{who} is publishing"));
                health
            }
            None => Health::degraded(format!(
                "nobody is publishing. Point an encoder at {} and the picture appears \
                 within a second or two of its first keyframe.",
                self.state.address()
            )),
        }
    }

    pub fn stats(&self) -> Value {
        serde_json::json!({
            "publishing": self.state.publisher.lock().unwrap_or_else(|e| e.into_inner()).clone(),
            "address": self.state.address(),
            "port": self.state.port.load(Ordering::Relaxed),
            "bytes": self.state.bytes.load(Ordering::Relaxed),
        })
    }
}

/// What a listener does when its publisher's stream has ended: a real source
/// finishes the Matroska and exits, so the core restarts it on a clean pipe
/// (`listen.rs` says why). A test writing to a file carries on.
fn ender(exit_at_end: bool) -> listen::End {
    Arc::new(move |remux: &Remux| {
        if exit_at_end {
            remux.finish();
            std::process::exit(0);
        }
    })
}

impl Drop for Ingest {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self._server = None;
        if let Some(thread) = self.reader.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
