//! `ingest/discover`: the channel server. One port per protocol, many
//! channels, many streams on each.
//!
//! This device holds the mixer's ingest ports, and only while a channel needs
//! them (`src/listeners.rs`): RTMP, SRT and RTMPS. WHIP arrives through the
//! core's control port (`src/whip.rs`). Each publisher is asked about by
//! the channel table (`src/channels.rs`, handed over by the core under
//! `channels` in the settings), and one that is let in feeds the hub
//! (`src/hub.rs`), where a mixer source and a restream read it through
//! bounded queues of their own.
//!
//! It reports what it carries three ways:
//!
//! 1. `event/channel.stream` and `event/channel.refused`, the moment a stream
//!    goes live, learns its codecs, leaves, or is turned away. The core's
//!    supervisor routes those to its channel registry, which is what makes a
//!    live stream a mixer source (`auto_source`).
//! 2. The `streams` tool answers with every live stream's codec, size, frame
//!    rate, bit rate, readers and dropped GOPs, read when a client asks.
//! 3. With no channels at all nothing listens, unless `open_door` is set:
//!    then the RTMP port is the open door it used to be, `discover` answers
//!    with one candidate per publisher, `event/ingest.publisher` is raised
//!    for each, and `add_publishers` makes the sources match.
//!
//! 4. `event/channel.destination` when one of a channel's destinations
//!    changes state, and its bit rate in the `streams` answer. Each
//!    destination the table carries is a restream reading the hub
//!    (`src/sends.rs`).
//!
//! Each source reads its stream from the hub over loopback on the RTMP port
//! number (`src/relay.rs`), which is bound on the loopback alone while no
//! channel has RTMP on, so an address handed to a source stays good whatever
//! protocols are switched on.

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::{Candidate, Health, ToolResult};
use serde_json::{json, Value};

use crate::channels::Table;
use crate::gate::ChannelGate;
use crate::hub::Hub;
use crate::listeners::Listeners;
use crate::rest::Core;
use crate::rtmp::slug;
use crate::sends::{Sends, Wanted};

/// The settings of `ingest/discover` that need the socket bound again.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub rtmp_port: u16,
    /// The one UDP port SRT takes every channel on.
    pub srt_port: u16,
    /// The first of the UDP ports WebRTC media uses, one per WHIP session.
    pub webrtc_port: u16,
    pub app: String,
    /// Take any RTMP publisher when there are no channels, as this device
    /// did before channels. Off unless a person turns it on.
    pub open_door: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            bind: "0.0.0.0".into(),
            rtmp_port: 1935,
            srt_port: 9000,
            webrtc_port: 8189,
            app: String::new(),
            open_door: false,
        }
    }
}

impl Settings {
    pub fn from_params(params: &Value) -> Settings {
        let d = Settings::default();
        let string = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        let port = |key: &str, default: u16| {
            params.get(key).and_then(Value::as_u64).and_then(|p| u16::try_from(p).ok()).unwrap_or(default)
        };
        Settings {
            bind: string("bind").unwrap_or(d.bind),
            rtmp_port: port("rtmp_port", d.rtmp_port),
            srt_port: port("srt_port", d.srt_port),
            webrtc_port: port("webrtc_port", d.webrtc_port),
            app: string("app").unwrap_or(d.app),
            open_door: params.get("open_door").and_then(Value::as_bool).unwrap_or(d.open_door),
        }
    }
}

/// The running device: the listener, the hub and the table.
pub struct Discover {
    gate: Arc<ChannelGate>,
    /// The ports, opened and closed as the table asks.
    listeners: Mutex<Listeners>,
    /// WHIP publishers, handed over by the core from its control port.
    pub whip: crate::whip::Sessions,
    /// Every slug `add_publishers` has added, so it knows what to remove.
    added: Mutex<Vec<String>>,
    /// The channels' destinations, each a restream reading the hub.
    sends: Sends,
    /// The shows with compositing off (`src/direct/`), on the same hub.
    pub direct: Arc<crate::direct::Host>,
}

impl Discover {
    pub fn start(settings: &Settings, table: Table, reporter: Option<Reporter>) -> Result<Discover, String> {
        let gate = Arc::new(ChannelGate {
            hub: Hub::new(),
            table: Arc::new(RwLock::new(table)),
            open_app: settings.app.clone(),
            relay: OnceLock::new(),
            reporter: reporter.clone(),
            on_air: Default::default(),
        });
        let mut listeners = Listeners::new(settings, gate.clone());
        listeners.apply(&gate.table.read().unwrap_or_else(|e| e.into_inner()));
        if let Some(r) = &reporter {
            let channels = gate.table.read().map(|t| t.channels.len()).unwrap_or(0);
            r.info(format!("{channels} channel(s); {}", listeners.summary()));
        }
        let sends = Sends::new(gate.hub.clone(), reporter.clone());
        let whip = crate::whip::Sessions::new(gate.clone(), settings.webrtc_port);
        let direct = direct_host(&gate, reporter);
        Ok(Discover { gate, listeners: Mutex::new(listeners), whip, added: Mutex::new(Vec::new()), sends, direct })
    }

    /// The RTMP port, bound or to be bound.
    pub fn port(&self) -> u16 {
        self.lock_listeners().rtmp_port()
    }

    fn lock_listeners(&self) -> std::sync::MutexGuard<'_, Listeners> {
        self.listeners.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// The hub every stream on this listener goes through. The restreamer
    /// reads a channel's streams with `hub().subscribe(app, stream)`.
    #[cfg(test)]
    pub fn hub(&self) -> &Hub {
        &self.gate.hub
    }

    /// Run the destinations the channel table asks for, and no others.
    pub fn set_sends(&self, wanted: Vec<Wanted>, specs: Vec<crate::transcode::StreamSpec>) {
        self.sends.apply(wanted, specs);
    }

    /// Take a new channel table. A publisher it no longer lets in (its key
    /// taken back, its channel or its protocol off, or gone) is cut off now,
    /// and the ports open and close to match.
    pub fn set_table(&self, table: Table) {
        *self.gate.table.write().unwrap_or_else(|e| e.into_inner()) = table;
        self.gate.enforce();
        let table = self.gate.table.read().unwrap_or_else(|e| e.into_inner()).clone();
        self.lock_listeners().apply(&table);
    }

    /// Every listener the channels asked for, open or not, and why.
    pub fn listeners(&self) -> Vec<Value> {
        let mut rows = self.lock_listeners().rows();
        rows.extend(self.whip.rows());
        rows
    }

    fn relay(&self) -> String {
        self.gate.relay.get().cloned().unwrap_or_default()
    }

    /// Run the direct shows the table asks for, and no others.
    pub fn set_direct(&self, params: &Value) {
        for why in self.direct.apply(params) {
            if let Some(r) = &self.gate.reporter {
                r.warn(why);
            }
        }
    }

    /// What `discover` answers with: one candidate per live channel stream.
    /// A direct show's input is the station's to make a source of.
    pub fn candidates(&self) -> Vec<Candidate> {
        self.gate
            .hub
            .streams()
            .iter()
            .filter(|s| !s["app"].as_str().unwrap_or("").starts_with("direct."))
            .map(|s| {
                let name = format!("{}/{}", s["app"].as_str().unwrap_or(""), s["stream"].as_str().unwrap_or(""));
                Candidate {
                    kind: "ingest/rtmp".into(),
                    params: json!({"relay": self.relay(), "stream": name}),
                    name,
                    confidence: 1.0,
                }
            })
            .collect()
    }

    /// The `streams` tool: every live stream, measured.
    pub fn streams(&self) -> ToolResult {
        let streams = self.gate.hub.streams();
        let port = self.port();
        let summary = format!("{} live stream(s); {}", streams.len(), self.lock_listeners().summary());
        let destinations = self.sends.rates();
        let listeners = self.listeners();
        ok_result(
            summary,
            json!({"port": port, "relay": self.relay(), "streams": streams, "destinations": destinations, "listeners": listeners}),
        )
    }

    pub fn health(&self) -> Health {
        let count = self.gate.hub.streams().len();
        let mut health = Health::ok();
        let ports = self.lock_listeners().summary();
        health.detail = Some(if count == 0 {
            format!("{ports}; nobody publishing")
        } else {
            format!("{count} publisher(s); {ports}")
        });
        health
    }

    /// The `add_publishers` tool: make the sources match the publishers.
    pub fn add_publishers(&self, arguments: &Value, core: Result<Core, String>) -> ToolResult {
        let dry_run = arguments.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
        let wanted: Vec<(String, Value)> = self
            .candidates()
            .into_iter()
            .map(|c| (slug(&c.name), json!({"id": slug(&c.name), "type": c.kind, "params": c.params})))
            .collect();
        let have: Vec<String> = self.added.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let to_add: Vec<&(String, Value)> = wanted.iter().filter(|(s, _)| !have.contains(s)).collect();
        let to_remove: Vec<String> =
            have.iter().filter(|s| !wanted.iter().any(|(w, _)| w == *s)).cloned().collect();
        let plan = json!({
            "add": to_add.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
            "remove": to_remove,
            "publishers": wanted.iter().map(|(s, _)| s.clone()).collect::<Vec<_>>(),
        });
        if dry_run {
            let summary = format!("would add {} source(s) and remove {}", to_add.len(), to_remove.len());
            return ok_result(summary, plan);
        }
        let core = match core {
            Ok(core) => core,
            Err(why) => return error_result(why),
        };
        let (mut done, mut failed) = (Vec::new(), Vec::new());
        for (slug, body) in &to_add {
            match core.add_source(body) {
                Ok(_) => {
                    self.added.lock().unwrap_or_else(|e| e.into_inner()).push(slug.clone());
                    done.push(slug.clone());
                }
                Err(e) => failed.push(format!("{slug}: {e}")),
            }
        }
        for slug in &to_remove {
            match core.remove_source(slug) {
                Ok(_) => {
                    self.added.lock().unwrap_or_else(|e| e.into_inner()).retain(|s| s != slug);
                    done.push(format!("-{slug}"));
                }
                Err(e) => failed.push(format!("{slug}: {e}")),
            }
        }
        let summary = if failed.is_empty() {
            format!("{} source(s) changed", done.len())
        } else {
            format!("{} source(s) changed, {} refused", done.len(), failed.len())
        };
        let mut result = ok_result(summary, json!({"changed": done, "failed": failed.clone()}));
        result.is_error = Some(!failed.is_empty());
        result
    }
}

/// The direct host on the gate's hub, telling the station through the
/// reporter, and naming the relay the gate binds.
fn direct_host(gate: &Arc<ChannelGate>, reporter: Option<Reporter>) -> Arc<crate::direct::Host> {
    let emit: crate::direct::Emit = Arc::new(move |name, params| {
        if let Some(r) = &reporter {
            r.event(name, params);
        }
    });
    let g = gate.clone();
    let relay: crate::direct::Relay = Arc::new(move || g.relay.get().cloned().unwrap_or_default());
    crate::direct::Host::new(gate.hub.clone(), emit, relay)
}

fn ok_result(summary: String, structured: Value) -> ToolResult {
    ToolResult {
        content: json!([{"type": "text", "text": summary}]),
        structured_content: Some(structured),
        is_error: Some(false),
    }
}

fn error_result(message: String) -> ToolResult {
    ToolResult {
        content: json!([{"type": "text", "text": message}]),
        structured_content: None,
        is_error: Some(true),
    }
}

#[cfg(test)]
#[path = "device_tests.rs"]
mod tests;
