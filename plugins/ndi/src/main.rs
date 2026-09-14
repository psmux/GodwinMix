//! `ndi/source`, `ndi/output` and `ndi/discover`.
//!
//! NDI is how a camera, a graphics machine and a mixer talk to each other on a
//! studio network without a capture card. It is also the one thing in this
//! repository that cannot be shipped: the runtime's licence forbids
//! redistribution, so this plugin links nothing NDI at build time, `dlopen`s
//! the runtime to see whether it is there, and refuses with the download page
//! when it is not.
//!
//! NDI is a trademark of Vizrt Group. This plugin is not affiliated with or
//! endorsed by Vizrt. See README.md.

mod library;
mod media;
mod senders;

use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::ToolResult;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// ndi/source
// ---------------------------------------------------------------------------

struct NdiSource {
    settings: media::SourceSettings,
    reporter: Option<Reporter>,
    receiver: Option<media::Receiver>,
}

impl Source for NdiSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = media::SourceSettings::from_params(&ready.params);
        if let Some(problem) = self.settings.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        check_ready_for_ndi(media::SOURCE_NEEDED)?;
        reporter.info(format!(
            "ndi/source '{}' will receive {}",
            ready.instance,
            self.settings.describe()
        ));
        self.reporter = Some(reporter);
        // NDI's own receive buffer, plus the frame in flight. The element does
        // not publish a number, so this is the honest order of magnitude.
        Ok(InitializeResult { latency_ms: Some(40) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        let receiver = media::Receiver::start(&self.settings, self.reporter.clone(), None)
            .map_err(internal)?;
        self.receiver = Some(receiver);
        Ok(StartResult { latency_ms: Some(40) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.receiver = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = media::SourceSettings::from_params(&params);
        if let Some(problem) = wanted.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.receiver.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "an NDI receiver is bound to one sender when it connects. Call plugin.reload \
             and it connects to the new one.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.receiver.as_ref() {
            Some(receiver) => receiver.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        shared_call("ndi/source", method, params)
    }
}

// ---------------------------------------------------------------------------
// ndi/output
// ---------------------------------------------------------------------------

struct NdiOutput {
    settings: media::OutputSettings,
    reporter: Option<Reporter>,
    announcer: Option<media::Announcer>,
}

impl Output for NdiOutput {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = media::OutputSettings::from_params(&ready.params);
        check_ready_for_ndi(media::OUTPUT_NEEDED)?;
        reporter.info(format!(
            "ndi/output '{}' will announce the programme as '{}'",
            ready.instance, self.settings.name
        ));
        self.reporter = Some(reporter);
        Ok(InitializeResult::default())
    }

    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        if params.media.trim().is_empty() {
            return Err(RpcError::new(
                codes::INVALID_PARAMS,
                "start.params.media is empty. An output reads the encoded programme from \
                 the FIFO the core names there. See docs/reference/plugin-lifecycle.md.",
            ));
        }
        let announcer =
            media::Announcer::start(&self.settings, &params.media, self.reporter.clone())
                .map_err(internal)?;
        self.announcer = Some(announcer);
        Ok(StartResult::default())
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.announcer = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = media::OutputSettings::from_params(&params);
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.announcer.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "an NDI sender announces its name once, when it starts. Call plugin.reload \
             and it announces the new one.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.announcer.as_ref() {
            Some(announcer) => announcer.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        shared_call("ndi/output", method, params)
    }
}

// ---------------------------------------------------------------------------
// ndi/discover
// ---------------------------------------------------------------------------

struct NdiDiscover;

impl Device for NdiDiscover {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        match library::find() {
            Ok(found) => reporter.info(format!(
                "ndi/discover '{}' is using the NDI runtime at {}",
                ready.instance, found.path
            )),
            // A device that cannot see anything is still worth having up: it
            // answers `discover` with nothing and says why in `health`, which
            // is more use than a dead instance.
            Err(why) => reporter.warn(why),
        }
        Ok(InitializeResult::default())
    }

    fn configure(&mut self, _params: Value) -> Result<Configure, RpcError> {
        Ok(Configure::applied())
    }

    fn health(&mut self) -> Health {
        match library::find() {
            Ok(found) => {
                let mut health = Health::ok();
                health.detail = Some(format!("the NDI runtime at {}", found.path));
                health
            }
            Err(why) => Health::degraded(why),
        }
    }

    fn discover(&mut self, timeout_ms: u64) -> Result<Vec<Candidate>, RpcError> {
        if library::find().is_err() {
            // Not an error: there is nothing wrong with a machine that has no
            // NDI on it, and `health` already says so.
            return Ok(Vec::new());
        }
        let found = senders::list(timeout_ms).map_err(internal)?;
        Ok(found
            .iter()
            .map(|sender| Candidate {
                kind: "ndi/source".into(),
                name: sender.name.clone(),
                params: json!({ "name": sender.name }),
                confidence: 1.0,
            })
            .collect())
    }

    fn call(&mut self, method: &str, params: Value) -> Result<Value, RpcError> {
        match method {
            "tool.call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let short = name.rsplit('/').next().unwrap_or(name);
                if short != "list_senders" {
                    return Err(RpcError::new(
                        codes::METHOD_NOT_FOUND,
                        format!(
                            "ndi has no tool '{name}'. It has one: list_senders, which \
                             lists every NDI sender visible on the network."
                        ),
                    ));
                }
                let timeout = params
                    .get("arguments")
                    .and_then(|a| a.get("timeout_ms"))
                    .and_then(Value::as_u64)
                    .unwrap_or(1500);
                serde_json::to_value(list_senders(timeout))
                    .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, e.to_string()))
            }
            other => Err(no_method("ndi/discover", other, "tool.call")),
        }
    }
}

/// The `list_senders` tool.
fn list_senders(timeout_ms: u64) -> ToolResult {
    if let Err(why) = library::find() {
        return ToolResult {
            content: json!([{"type": "text", "text": why}]),
            structured_content: Some(json!({"senders": []})),
            is_error: Some(true),
        };
    }
    match senders::list(timeout_ms) {
        Ok(found) => {
            let list: Vec<Value> = found.iter().map(|s| s.json()).collect();
            let summary = if list.is_empty() {
                "no NDI senders are visible on this network. Check the sender is running \
                 and that both machines are on the same subnet: NDI finds senders with \
                 mDNS, which does not cross a router."
                    .to_string()
            } else {
                format!(
                    "{} NDI sender(s): {}",
                    list.len(),
                    found
                        .iter()
                        .map(|s| s.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            ToolResult {
                content: json!([{"type": "text", "text": summary}]),
                structured_content: Some(json!({ "senders": list })),
                is_error: Some(false),
            }
        }
        Err(why) => ToolResult {
            content: json!([{"type": "text", "text": why}]),
            structured_content: Some(json!({"senders": []})),
            is_error: Some(true),
        },
    }
}

// ---------------------------------------------------------------------------

/// Both media provides answer `stats` the same way.
fn shared_call(provide: &str, method: &str, _params: Value) -> Result<Value, RpcError> {
    match method {
        "stats" => Ok(json!({
            "runtime": match library::find() {
                Ok(found) => json!({"found": true, "path": found.path}),
                Err(why) => json!({"found": false, "detail": why}),
            },
            "elements": library::elements_present(),
        })),
        other => Err(no_method(provide, other, "stats")),
    }
}

/// Refuse early and clearly when either half of NDI is missing.
fn check_ready_for_ndi(needed: &[&str]) -> Result<(), RpcError> {
    gmx_netkit::init().map_err(internal)?;
    gmx_netkit::elements::require(needed).map_err(internal)?;
    library::find().map_err(internal)?;
    Ok(())
}

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
        // Run by hand, this is the most useful thing it can do: say whether the
        // runtime is here, and list what it can see.
        eprintln!("gmx-ndi is a GodwinMix plugin: the core starts it and talks JSON lines");
        eprintln!("on stdin and stderr. Install it with `gmx plugin add plugins/ndi`.\n");
        match library::find() {
            Ok(found) => {
                eprintln!("the NDI runtime is at {}", found.path);
                match senders::list(1500) {
                    Ok(found) if found.is_empty() => eprintln!("no senders are visible"),
                    Ok(found) => {
                        for sender in found {
                            eprintln!("  {}  {}", sender.name, sender.address);
                        }
                    }
                    Err(why) => eprintln!("{why}"),
                }
                std::process::exit(2);
            }
            Err(why) => {
                eprintln!("{why}");
                std::process::exit(3);
            }
        }
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-ndi could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let provide = env.provide.clone();
    let outcome = match provide.as_str() {
        "output" => runtime::run(
            &manifest,
            OutputHandler(NdiOutput {
                settings: media::OutputSettings::default(),
                reporter: None,
                announcer: None,
            }),
        ),
        "discover" => runtime::run(&manifest, DeviceHandler(NdiDiscover)),
        _ => runtime::run(
            &manifest,
            SourceHandler(NdiSource {
                settings: media::SourceSettings::default(),
                reporter: None,
                receiver: None,
            }),
        ),
    };
    if let Err(e) = outcome {
        eprintln!("ndi/{provide} stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::{Canvas, Transport};

    fn ready(params: Value) -> Ready {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.instance = "cam1".into();
        ready.provide = "source".into();
        ready.transport = Transport::Container;
        ready.params = params;
        ready
    }

    #[test]
    fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = Manifest::load(root.join("gmx-plugin.toml"))
            .expect("the manifest beside this crate must load and validate");
        assert_eq!(manifest.plugin.name, "ndi");
        for id in ["source", "output", "discover"] {
            assert!(manifest.provides.iter().any(|p| p.id == id), "no provide '{id}'");
        }
        assert!(manifest.tools.iter().any(|t| t.name == "list_senders"));
    }

    #[test]
    fn a_bandwidth_that_does_not_exist_is_refused_at_initialize() {
        let mut plugin = NdiSource {
            settings: media::SourceSettings::default(),
            reporter: None,
            receiver: None,
        };
        let err = match plugin.initialize(
            &ready(json!({"name": "CAM", "bandwidth": "medium"})),
            Reporter::for_test(),
        ) {
            Ok(_) => panic!("there is no medium bandwidth"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn a_machine_with_no_runtime_refuses_at_initialize_with_the_download_page() {
        if library::find().is_ok() {
            eprintln!("skipping: this machine has the NDI runtime");
            return;
        }
        let mut plugin = NdiSource {
            settings: media::SourceSettings::default(),
            reporter: None,
            receiver: None,
        };
        let err = match plugin.initialize(&ready(json!({"name": "CAM"})), Reporter::for_test()) {
            Ok(_) => panic!("there is no runtime on this machine"),
            Err(e) => e,
        };
        assert!(err.message.contains("ndi.video"), "{}", err.message);
    }

    #[test]
    fn the_device_stays_up_without_a_runtime_and_says_so() {
        let mut device = NdiDiscover;
        device
            .initialize(&ready(json!({})), Reporter::for_test())
            .expect("a device with nothing to find is still worth having up");
        let health = device.health();
        if library::find().is_ok() {
            assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Ok);
        } else {
            assert_eq!(health.state, godwinmix_sdk::wire::HealthState::Degraded);
            assert!(health.detail.unwrap_or_default().contains("ndi.video"));
            assert!(device.discover(200).expect("an empty answer, not an error").is_empty());
        }
    }

    #[test]
    fn list_senders_on_a_machine_with_no_runtime_is_an_error_that_names_the_download() {
        let result = list_senders(200);
        if library::find().is_ok() {
            assert_eq!(result.is_error, Some(false));
        } else {
            assert_eq!(result.is_error, Some(true));
            assert!(result.content.to_string().contains("ndi.video"));
        }
    }

    #[test]
    fn the_device_refuses_a_tool_it_does_not_have_and_names_the_one_it_does() {
        let err = NdiDiscover
            .call("tool.call", json!({"name": "ndi/nope", "arguments": {}}))
            .unwrap_err();
        assert!(err.message.contains("list_senders"), "{}", err.message);
    }

    #[test]
    fn stats_say_whether_each_half_of_ndi_is_present() {
        let answer = shared_call("ndi/source", "stats", Value::Null).expect("stats always answer");
        assert!(answer["runtime"]["found"].is_boolean());
        assert!(answer["elements"].is_boolean());
    }
}
