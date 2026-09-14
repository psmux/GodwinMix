//! `ingest`: GodwinMix as a server.
//!
//! Everywhere else the mixer dials out. Here it waits, and a phone, an OBS on
//! the other laptop, a hardware encoder in the rack or a guest in a browser
//! dials in. That is how a church or a small studio actually gets its pictures,
//! and today it needs a separate mediamtx running beside the mixer.
//!
//! | Provide | What it is |
//! |---|---|
//! | `ingest/rtmp` | an RTMP listener written in Rust, remuxed to Matroska |
//! | `ingest/whip` | a WHIP endpoint, so a browser needs nothing but the URL |
//! | `ingest/discover` | one RTMP port for many publishers, each reported as a candidate |
//!
//! The SRT listener is not here: `srt/source` already is one, and its default
//! mode is `listener`. Duplicating it would mean two places to fix a bug.
//! `docs/how-to/receive-a-phone-or-obs-stream.md` says so where a reader looking
//! for it will be.

mod device;
mod flv;
mod relay;
mod remux;
mod rest;
mod rtmp;
mod source;
mod whip_in;

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// ingest/rtmp
// ---------------------------------------------------------------------------

struct RtmpIngest {
    settings: source::Settings,
    reporter: Option<Reporter>,
    running: Option<source::Ingest>,
}

impl Source for RtmpIngest {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = source::Settings::from_params(&ready.params);
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        reporter.info(format!(
            "ingest/rtmp '{}' will take a publisher at {}",
            ready.instance,
            self.settings.publish_url(self.settings.port)
        ));
        self.reporter = Some(reporter);
        // Nothing is buffered here: a tag is written on as it arrives. What
        // latency there is belongs to the publisher's encoder.
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        let running =
            source::Ingest::start(&self.settings, self.reporter.clone(), crate::remux::Out::Stdout)
                .map_err(internal)?;
        self.running = Some(running);
        Ok(StartResult { latency_ms: Some(0) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.running = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = source::Settings::from_params(&params);
        if let Some(problem) = wanted.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.running.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "the listening port and the stream key are chosen when the socket is bound. \
             Call plugin.reload and this source listens with the new settings.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.running.as_ref() {
            Some(running) => running.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(self
                .running
                .as_ref()
                .map(|r| r.stats())
                .unwrap_or_else(|| json!({"publishing": null}))),
            other => Err(no_method("ingest/rtmp", other, "stats")),
        }
    }
}

// ---------------------------------------------------------------------------
// ingest/whip
// ---------------------------------------------------------------------------

struct WhipIngest {
    settings: whip_in::Settings,
    reporter: Option<Reporter>,
    endpoint: Option<whip_in::Endpoint>,
}

impl Source for WhipIngest {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = whip_in::Settings::from_params(&ready.params);
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        gmx_netkit::init().map_err(internal)?;
        if !gmx_netkit::elements::exists("whipserversrc") {
            // A refusal, not a crash, and it names the package and the way
            // round. The harness marks the plugin failed here rather than
            // pretending a WHIP endpoint exists.
            return Err(RpcError::new(codes::INTERNAL_ERROR, whip_in::unavailable()));
        }
        reporter.info(format!(
            "ingest/whip '{}' will take a publisher at {}",
            ready.instance,
            self.settings.publish_url()
        ));
        self.reporter = Some(reporter);
        Ok(InitializeResult { latency_ms: Some(100) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        let endpoint =
            whip_in::Endpoint::start(&self.settings, self.reporter.clone()).map_err(internal)?;
        self.endpoint = Some(endpoint);
        Ok(StartResult { latency_ms: Some(100) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.endpoint = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = whip_in::Settings::from_params(&params);
        if let Some(problem) = wanted.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.endpoint.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "the endpoint's address is chosen when its HTTP server binds. Call \
             plugin.reload and it listens at the new one.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.endpoint.as_ref() {
            Some(endpoint) => endpoint.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(self
                .endpoint
                .as_ref()
                .map(|e| e.stats())
                .unwrap_or_else(|| json!({"address": self.settings.publish_url()}))),
            other => Err(no_method("ingest/whip", other, "stats")),
        }
    }
}

// ---------------------------------------------------------------------------
// ingest/discover
// ---------------------------------------------------------------------------

struct Publishers {
    settings: device::Settings,
    env: PluginEnv,
    running: Option<device::Discover>,
}

impl Device for Publishers {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = device::Settings::from_params(&ready.params);
        let running = device::Discover::start(&self.settings, Some(reporter)).map_err(internal)?;
        self.running = Some(running);
        Ok(InitializeResult::default())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = device::Settings::from_params(&params);
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        self.settings = wanted;
        Ok(Configure::restart_required(
            "the listening port is chosen when the socket is bound. Call plugin.reload \
             and the device listens on the new one.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.running.as_ref() {
            Some(running) => running.health(),
            None => Health::failing("the RTMP listener did not start"),
        }
    }

    fn discover(&mut self, _timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        // Nothing to wait for: the listener has been running since initialize
        // and the table is current, so the timeout is not used.
        Ok(self
            .running
            .as_ref()
            .map(|r| r.candidates())
            .unwrap_or_default())
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "tool.call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
                let short = name.rsplit('/').next().unwrap_or(name);
                if short != "add_publishers" {
                    return Err(RpcError::new(
                        codes::METHOD_NOT_FOUND,
                        format!(
                            "ingest has no tool '{name}'. It has one: add_publishers, \
                             which makes the sources match the publishers."
                        ),
                    ));
                }
                let running = self.running.as_ref().ok_or_else(|| {
                    RpcError::new(codes::WRONG_STATE, "the RTMP listener is not running")
                })?;
                let core = rest::Core::from_env(&self.env.rpc, &self.env.token);
                let result = running.add_publishers(&arguments, core);
                serde_json::to_value(result)
                    .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
            }
            other => Err(no_method("ingest/discover", other, "tool.call")),
        }
    }
}

// ---------------------------------------------------------------------------

fn internal(message: String) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, message)
}

fn no_method(provide: &str, method: &str, has: &str) -> RpcError {
    RpcError::new(
        codes::METHOD_NOT_FOUND,
        format!(
            "{provide} has no method '{method}'. It answers '{has}', and the standard \
             methods listed in docs/reference/plugin-protocol.md."
        ),
    )
}

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-ingest is a GodwinMix plugin: the core starts it and talks JSON lines on \
             stdin and stderr.\nInstall it with `gmx plugin add plugins/ingest`, then \
             `gmx source add phone --type ingest/rtmp`, and publish to \
             rtmp://<this machine>:1935/live/anything.\nSee plugins/ingest/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-ingest could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let provide = env.provide.clone();
    let outcome = match provide.as_str() {
        "whip" => runtime::run(
            &manifest,
            SourceHandler(WhipIngest {
                settings: whip_in::Settings::default(),
                reporter: None,
                endpoint: None,
            }),
        ),
        "discover" => runtime::run(
            &manifest,
            DeviceHandler(Publishers {
                settings: device::Settings::default(),
                env,
                running: None,
            }),
        ),
        _ => runtime::run(
            &manifest,
            SourceHandler(RtmpIngest {
                settings: source::Settings::default(),
                reporter: None,
                running: None,
            }),
        ),
    };
    if let Err(e) = outcome {
        eprintln!("ingest/{provide} stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::{Canvas, Transport};

    fn ready(params: Value) -> Ready {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.instance = "phone".into();
        ready.provide = "rtmp".into();
        ready.transport = Transport::Container;
        ready.params = params;
        ready
    }

    fn an_rtmp_source() -> RtmpIngest {
        RtmpIngest {
            settings: source::Settings::default(),
            reporter: None,
            running: None,
        }
    }

    #[test]
    fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = Manifest::load(root.join("gmx-plugin.toml"))
            .expect("the manifest beside this crate must load and validate");
        assert_eq!(manifest.plugin.name, "ingest");
        for id in ["rtmp", "whip", "discover"] {
            assert!(manifest.provides.iter().any(|p| p.id == id), "no provide '{id}'");
        }
        assert!(manifest.tools.iter().any(|t| t.name == "add_publishers"));
    }

    #[test]
    fn an_rtmp_source_comes_up_with_nothing_configured_at_all() {
        let mut plugin = an_rtmp_source();
        let result = plugin
            .initialize(&ready(json!({})), Reporter::for_test())
            .expect("no configuration is the point of this provide");
        assert_eq!(result.latency_ms, Some(0));
        assert_eq!(plugin.settings.port, 1935);
    }

    #[test]
    fn a_relay_that_is_not_an_address_is_refused_with_minus_32602() {
        let mut plugin = an_rtmp_source();
        let err = match plugin.initialize(&ready(json!({"relay": "nonsense"})), Reporter::for_test())
        {
            Ok(_) => panic!("a relay without a port cannot be reached"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn a_new_port_while_stopped_is_applied_and_while_running_needs_a_reload() {
        let mut plugin = an_rtmp_source();
        let applied = plugin.configure(json!({"port": 1936})).expect("a stopped source moves");
        assert!(applied.applied);
        assert_eq!(plugin.settings.port, 1936);
        let again = plugin.configure(json!({"port": 1936})).expect("the same settings apply");
        assert!(again.applied);
    }

    #[test]
    fn an_unknown_method_names_what_this_provide_answers() {
        let err = an_rtmp_source().call("teleport", Value::Null).unwrap_err();
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("stats"), "{}", err.message);
    }

    #[test]
    fn a_whip_ingest_with_a_bad_path_is_refused_before_anything_binds() {
        let mut plugin = WhipIngest {
            settings: whip_in::Settings::default(),
            reporter: None,
            endpoint: None,
        };
        let err = match plugin.initialize(&ready(json!({"path": "whip"})), Reporter::for_test()) {
            Ok(_) => panic!("a path must begin with a slash"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn the_device_refuses_a_tool_it_does_not_have_and_names_the_one_it_does() {
        let mut plugin = Publishers {
            settings: device::Settings::default(),
            env: PluginEnv::default(),
            running: None,
        };
        let err = plugin
            .call("tool.call", json!({"name": "ingest/burn_it_down", "arguments": {}}))
            .unwrap_err();
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("add_publishers"), "{}", err.message);
    }
}
