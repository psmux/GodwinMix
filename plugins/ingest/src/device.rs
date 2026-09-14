//! `ingest/discover`: one address, many publishers, sources that appear.
//!
//! This is the provide that makes the acceptance line in the roadmap literal:
//! an OBS or a phone publishing RTMP to the mixer's address appears as a live
//! source within five seconds with no configuration.
//!
//! It holds the RTMP port itself, so several publishers can arrive on one
//! address, and gives each one a loopback relay an `ingest/rtmp` source reads
//! from. It reports them three ways:
//!
//! 1. `discover` answers with one candidate per publisher, ready for
//!    `source.add`. This is the method the core calls on a device provide.
//! 2. `event/ingest.publisher` notifications go out the moment a publisher
//!    connects or leaves, so a core that wants to add and remove sources by
//!    itself has something to act on.
//! 3. The `add_publishers` tool does the adding and removing itself, through
//!    the core's public REST layer.
//!
//! # What the core cannot do yet
//!
//! All three are written to the contract and none of them is reachable in the
//! core as it stands. Exactly what is missing:
//!
//! * `crates/godwinmix-core/src/plugin/loader.rs` interns only provides whose
//!   `kind` is `"source"`, so a `device` provide is parsed, listed, and then
//!   registers nothing. `SidecarDevice` exists in
//!   `crates/godwinmix-core/src/plugin/host/service.rs` and is constructed
//!   nowhere.
//! * No RPC method, CLI command or timer calls `discover`. There is no
//!   `device.discover` in the method table.
//! * A plugin's `event` notification is parsed by
//!   `crates/godwinmix-core/src/plugin/host/process.rs` into a bounded ring
//!   buffer that nothing reads. `notices()` has no caller.
//! * `tool.call` is not a registered method, so the MCP bridge's
//!   `POST /api/v1/tool/call` cannot reach a plugin's tool.
//!
//! Until those are wired, the working path is an `ingest/rtmp` source that owns
//! its own port: add it once and every publisher that arrives is live within
//! five seconds with nothing else to configure. `docs/how-to/receive-a-phone-or-obs-stream.md`
//! leads with that and says why.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use godwinmix_sdk::plugin::Reporter;
use godwinmix_sdk::wire::{Candidate, Health, ToolResult};
use serde_json::{json, Value};

use crate::relay::Relay;
use crate::rest::Core;
use crate::rtmp::{Event, Filter, Publisher, Server, Sink};

/// The settings of `ingest/discover`.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    pub bind: String,
    pub rtmp_port: u16,
    pub app: String,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            bind: "0.0.0.0".into(),
            rtmp_port: 1935,
            app: String::new(),
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

/// One publisher and the relay carrying its stream.
struct Live {
    publisher: Publisher,
    relay: Relay,
    /// True until the codec headers have been kept for a late reader.
    wants_headers: bool,
}

/// The table of who is publishing right now.
#[derive(Default)]
struct Registry {
    live: Mutex<HashMap<String, Live>>,
}

impl Registry {
    /// The candidates `discover` answers with.
    fn candidates(&self) -> Vec<Candidate> {
        let held = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<Candidate> = held
            .values()
            .map(|live| Candidate {
                kind: "ingest/rtmp".into(),
                name: format!("{}/{}", live.publisher.app, live.publisher.key),
                params: json!({ "relay": live.relay.address() }),
                confidence: 1.0,
            })
            .collect();
        out.sort_by(|a, b| a.name.cmp(&b.name));
        out
    }

    /// The slug and candidate for each publisher, for the tool.
    fn wanted(&self) -> Vec<(String, Value)> {
        let held = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let mut out: Vec<(String, Value)> = held
            .values()
            .map(|live| {
                (
                    live.publisher.slug(),
                    json!({
                        "id": live.publisher.slug(),
                        "type": "ingest/rtmp",
                        "params": { "relay": live.relay.address() },
                    }),
                )
            })
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

/// The running device: the listener, the relays, and the table.
pub struct Discover {
    registry: Arc<Registry>,
    settings: Settings,
    port: u16,
    /// Held so the listener lives as long as the device does.
    _server: Server,
    /// Every slug this device has ever added, so it knows what to remove.
    added: Arc<Mutex<Vec<String>>>,
    _stop: Arc<AtomicBool>,
}

impl Discover {
    pub fn start(settings: &Settings, reporter: Option<Reporter>) -> Result<Discover, String> {
        let registry = Arc::new(Registry::default());
        let sink = sink_for(Arc::clone(&registry), reporter.clone());
        let filter = Filter {
            app: settings.app.clone(),
            key: String::new(),
            // Many publishers on one port is the whole point of this provide.
            one_at_a_time: false,
        };
        let server = Server::bind(&settings.bind, settings.rtmp_port, filter, sink)?;
        let port = server.port();
        if let Some(r) = &reporter {
            r.info(format!(
                "listening for RTMP publishers on port {port}. Each one that arrives is \
                 reported by discover and by event/ingest.publisher."
            ));
        }
        Ok(Discover {
            registry,
            settings: settings.clone(),
            port,
            _server: server,
            added: Arc::new(Mutex::new(Vec::new())),
            _stop: Arc::new(AtomicBool::new(false)),
        })
    }

    #[cfg(test)]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// What `discover` answers with.
    pub fn candidates(&self) -> Vec<Candidate> {
        self.registry.candidates()
    }

    pub fn health(&self) -> Health {
        let count = self.registry.live.lock().unwrap_or_else(|e| e.into_inner()).len();
        let mut health = Health::ok();
        health.detail = Some(if count == 0 {
            format!(
                "listening on rtmp://<this machine>:{}/{}/<key>, nobody publishing",
                self.port,
                if self.settings.app.is_empty() { "<any app>" } else { &self.settings.app }
            )
        } else {
            format!("{count} publisher(s) on port {}", self.port)
        });
        health
    }

    /// The `add_publishers` tool: make the sources match the publishers.
    ///
    /// Adds an `ingest/rtmp` source for every publisher that has none, removes
    /// the ones this device added whose publisher has gone, and leaves
    /// everything else alone. `dry_run` answers with the plan and changes
    /// nothing.
    pub fn add_publishers(&self, arguments: &Value, core: Result<Core, String>) -> ToolResult {
        let dry_run = arguments
            .get("dry_run")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let wanted = self.registry.wanted();
        let have: Vec<String> = self.added.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let to_add: Vec<&(String, Value)> =
            wanted.iter().filter(|(slug, _)| !have.contains(slug)).collect();
        let to_remove: Vec<String> = have
            .iter()
            .filter(|slug| !wanted.iter().any(|(w, _)| w == *slug))
            .cloned()
            .collect();

        let plan = json!({
            "add": to_add.iter().map(|(slug, _)| slug.clone()).collect::<Vec<_>>(),
            "remove": to_remove.clone(),
            "publishers": wanted.iter().map(|(slug, _)| slug.clone()).collect::<Vec<_>>(),
        });
        if dry_run {
            return ok_result(
                format!(
                    "would add {} source(s) and remove {}",
                    to_add.len(),
                    to_remove.len()
                ),
                plan,
            );
        }
        let core = match core {
            Ok(core) => core,
            Err(why) => return error_result(why),
        };

        let mut done: Vec<String> = Vec::new();
        let mut failed: Vec<String> = Vec::new();
        for (slug, body) in &to_add {
            match core.add_source(body) {
                Ok(_) => {
                    self.added
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(slug.clone());
                    done.push(slug.clone());
                }
                Err(e) => failed.push(format!("{slug}: {e}")),
            }
        }
        for slug in &to_remove {
            match core.remove_source(slug) {
                Ok(_) => {
                    let mut held = self.added.lock().unwrap_or_else(|e| e.into_inner());
                    held.retain(|s| s != slug);
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

/// Keep the table up to date and push an event for every arrival and departure.
fn sink_for(registry: Arc<Registry>, reporter: Option<Reporter>) -> Sink {
    Arc::new(move |event| match event {
        Event::Arrived { app, key, peer } => {
            let publisher = Publisher { app: app.clone(), key: key.clone(), peer: peer.clone() };
            let Ok(relay) = Relay::open() else {
                if let Some(r) = &reporter {
                    r.error(format!(
                        "no loopback port was free for {app}/{key}, so it cannot be handed \
                         to a source. It is still connected; try again."
                    ));
                }
                return;
            };
            let address = relay.address();
            let slug = publisher.slug();
            registry.live.lock().unwrap_or_else(|e| e.into_inner()).insert(
                format!("{app}/{key}"),
                Live { publisher, relay, wants_headers: true },
            );
            if let Some(r) = &reporter {
                r.info(format!("{app}/{key} from {peer} is publishing, relayed at {address}"));
                r.event(
                    "ingest.publisher",
                    json!({
                        "action": "connected",
                        "id": slug,
                        "type": "ingest/rtmp",
                        "name": format!("{app}/{key}"),
                        "peer": peer,
                        "params": {"relay": address},
                    }),
                );
            }
        }
        Event::Bytes(bytes) => {
            let held = registry.live.lock().unwrap_or_else(|e| e.into_inner());
            // One publisher at a time is the common case and the loop is over a
            // handful of entries, so this costs nothing worth optimising.
            for live in held.values() {
                if live.wants_headers {
                    live.relay.remember(&bytes);
                }
                live.relay.send(&bytes);
            }
        }
        Event::Left { app, key } => {
            let gone = registry
                .live
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(&format!("{app}/{key}"));
            let Some(gone) = gone else { return };
            if let Some(r) = &reporter {
                r.info(format!("{app}/{key} stopped publishing"));
                r.event(
                    "ingest.publisher",
                    json!({
                        "action": "left",
                        "id": gone.publisher.slug(),
                        "name": format!("{app}/{key}"),
                    }),
                );
            }
        }
        Event::Note(message) => {
            if let Some(r) = &reporter {
                r.warn(message);
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_device() -> Discover {
        let settings = Settings::from_params(&json!({"bind": "127.0.0.1", "rtmp_port": 0}));
        Discover::start(&settings, None).expect("the loopback has a free port")
    }

    #[test]
    fn a_device_binds_and_reports_that_nobody_is_publishing() {
        let device = a_device();
        assert!(device.port() > 0);
        assert!(device.candidates().is_empty());
        let health = device.health();
        assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Ok);
        assert!(health.detail.unwrap_or_default().contains("nobody publishing"));
    }

    #[test]
    fn a_publisher_in_the_table_becomes_a_candidate_ready_for_source_add() {
        let device = a_device();
        let relay = Relay::open().expect("a relay port");
        let address = relay.address();
        device.registry.live.lock().unwrap().insert(
            "live/phone".into(),
            Live {
                publisher: Publisher {
                    app: "live".into(),
                    key: "phone".into(),
                    peer: "10.0.0.9:51000".into(),
                },
                relay,
                wants_headers: false,
            },
        );
        let candidates = device.candidates();
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].kind, "ingest/rtmp");
        assert_eq!(candidates[0].name, "live/phone");
        assert_eq!(candidates[0].params["relay"], address);
        assert_eq!(device.health().detail.unwrap_or_default(), format!("1 publisher(s) on port {}", device.port()));
    }

    #[test]
    fn a_dry_run_says_what_it_would_do_and_needs_no_core() {
        let device = a_device();
        let relay = Relay::open().expect("a relay port");
        device.registry.live.lock().unwrap().insert(
            "live/phone".into(),
            Live {
                publisher: Publisher {
                    app: "live".into(),
                    key: "phone".into(),
                    peer: "10.0.0.9:51000".into(),
                },
                relay,
                wants_headers: false,
            },
        );
        let result = device.add_publishers(
            &json!({"dry_run": true}),
            Err("no core in a test".to_string()),
        );
        assert_eq!(result.is_error, Some(false));
        let plan = result.structured_content.expect("a plan");
        assert_eq!(plan["add"], json!(["live-phone"]));
        assert_eq!(plan["remove"], json!([]));
    }

    #[test]
    fn a_real_run_with_no_core_reachable_is_an_error_that_says_why() {
        let device = a_device();
        let result = device.add_publishers(&json!({}), Err("GMX_RPC is empty".to_string()));
        assert_eq!(result.is_error, Some(true));
        assert!(result.content.to_string().contains("GMX_RPC"));
    }
}
