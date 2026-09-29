//! `ingest/discover`: the channel server. One RTMP port, many channels, many
//! streams on each.
//!
//! This device holds the mixer's RTMP port. Each publisher is asked about by
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
//! 3. With no channels at all it is the open door it always was: `discover`
//!    answers with one candidate per publisher, `event/ingest.publisher` is
//!    raised for each, and `add_publishers` makes the sources match.
//!
//! Each source reads its stream from the hub over loopback on this same port
//! (`src/relay.rs`), so an address handed to a source stays good for as long
//! as the port does.

use std::sync::{Arc, Mutex, OnceLock, RwLock};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::{Candidate, Health, ToolResult};
use serde_json::{json, Value};

use crate::channels::Table;
use crate::gate::ChannelGate;
use crate::hub::Hub;
use crate::rest::Core;
use crate::rtmp::{slug, Server};

/// The settings of `ingest/discover` that need the socket bound again.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub rtmp_port: u16,
    pub app: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings { bind: "0.0.0.0".into(), rtmp_port: 1935, app: String::new() }
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
        Settings {
            bind: string("bind").unwrap_or(d.bind),
            rtmp_port: params
                .get("rtmp_port")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok())
                .unwrap_or(d.rtmp_port),
            app: string("app").unwrap_or(d.app),
        }
    }
}

/// The running device: the listener, the hub and the table.
pub struct Discover {
    gate: Arc<ChannelGate>,
    port: u16,
    /// Held so the listener lives as long as the device does.
    _server: Server,
    /// Every slug `add_publishers` has added, so it knows what to remove.
    added: Mutex<Vec<String>>,
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
        let server = Server::bind(&settings.bind, settings.rtmp_port, gate.clone())?;
        let port = server.port();
        let _ = gate.relay.set(format!("127.0.0.1:{port}"));
        if let Some(r) = &reporter {
            let channels = gate.table.read().map(|t| t.channels.len()).unwrap_or(0);
            r.info(format!("listening for RTMP publishers on port {port}, {channels} channel(s)"));
        }
        Ok(Discover { gate, port, _server: server, added: Mutex::new(Vec::new()) })
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }

    #[cfg(test)]
    pub fn hub(&self) -> &Hub {
        &self.gate.hub
    }

    /// Take a new channel table. A publisher it no longer lets in (its key
    /// taken back, its channel off or gone) is cut off now.
    pub fn set_table(&self, table: Table) {
        *self.gate.table.write().unwrap_or_else(|e| e.into_inner()) = table;
        self.gate.enforce();
    }

    fn relay(&self) -> String {
        self.gate.relay.get().cloned().unwrap_or_default()
    }

    /// What `discover` answers with: one candidate per live stream.
    pub fn candidates(&self) -> Vec<Candidate> {
        self.gate
            .hub
            .streams()
            .iter()
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
        let summary = format!("{} live stream(s) on port {}", streams.len(), self.port);
        ok_result(summary, json!({"port": self.port, "relay": self.relay(), "streams": streams}))
    }

    pub fn health(&self) -> Health {
        let count = self.gate.hub.streams().len();
        let mut health = Health::ok();
        health.detail = Some(if count == 0 {
            format!("listening on rtmp://<this machine>:{}, nobody publishing", self.port)
        } else {
            format!("{count} publisher(s) on port {}", self.port)
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
