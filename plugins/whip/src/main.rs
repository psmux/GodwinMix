//! `whip/output` and `whip/whep`: WebRTC in and out, over HTTP.
//!
//! WHIP is one HTTP POST carrying an SDP offer. That is the whole protocol, and
//! it is why a browser, a phone or a cloud service can take a stream from this
//! mixer without a signalling server anybody has to run. WHEP is the same thing
//! pointed the other way, so this plugin carries both: `whip/output` sends the
//! programme, `whip/whep` receives somebody else's stream as a source.
//!
//! One process serves one provide. `GMX_PROVIDE` says which, and `main` picks
//! the handler from it.

mod output;
mod settings;
mod whep;

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use output::Sender;
use settings::Settings;
use whep::Watcher;

// ---------------------------------------------------------------------------
// whip/output
// ---------------------------------------------------------------------------

struct WhipOutput {
    settings: Settings,
    reporter: Option<Reporter>,
    sender: Option<Sender>,
}

impl Output for WhipOutput {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params);
        if let Some(problem) = self.settings.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(output::NEEDED).map_err(internal)?;
        reporter.info(format!(
            "whip/output '{}' will send the programme to {} ({})",
            ready.instance,
            self.settings.redacted_endpoint(),
            if self.settings.has_token() { "with a token" } else { "no token" }
        ));
        self.reporter = Some(reporter);
        Ok(InitializeResult::default())
    }

    /// `params.media` is the FIFO the core has already started muxing the
    /// programme into. An output reads it; it is the one place the media flows
    /// towards the plugin.
    fn start(&mut self, params: &StartParams) -> Result<StartResult, RpcError> {
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if params.media.trim().is_empty() {
            return Err(RpcError::new(
                codes::INVALID_PARAMS,
                "start.params.media is empty. An output reads the encoded programme from \
                 the FIFO the core names there (and in GMX_MEDIA); without it there is \
                 nothing to send. See docs/reference/plugin-lifecycle.md.",
            ));
        }
        let sender = Sender::start(&self.settings, &params.media, self.reporter.clone())
            .map_err(internal)?;
        self.sender = Some(sender);
        Ok(StartResult::default())
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.sender = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params);
        if let Some(problem) = wanted.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.sender.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "a WHIP session is one HTTP POST to one endpoint with one token. \
             Call plugin.reload and the output posts again with the new settings.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.sender.as_ref() {
            Some(sender) => sender.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({
                "endpoint": self.settings.redacted_endpoint(),
                "token": self.settings.has_token(),
                "reconnects": self.sender.as_ref().map(|s| s.reconnects()).unwrap_or(0),
                "health": self.sender.as_ref().map(|s| s.health().detail).unwrap_or(None),
            })),
            other => Err(no_method("whip/output", other)),
        }
    }
}

// ---------------------------------------------------------------------------
// whip/whep
// ---------------------------------------------------------------------------

struct WhepSource {
    settings: Settings,
    reporter: Option<Reporter>,
    watcher: Option<Watcher>,
}

impl Source for WhepSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params);
        if let Some(problem) = self.settings.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(whep::NEEDED).map_err(internal)?;
        reporter.info(format!(
            "whip/whep '{}' will watch {}",
            ready.instance,
            self.settings.redacted_endpoint()
        ));
        self.reporter = Some(reporter);
        // A WebRTC jitter buffer is what this source adds, and the element
        // decides it per stream. 100 ms is the honest order of magnitude.
        Ok(InitializeResult { latency_ms: Some(100) })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        let watcher = Watcher::start(&self.settings, self.reporter.clone()).map_err(internal)?;
        self.watcher = Some(watcher);
        Ok(StartResult { latency_ms: Some(100) })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.watcher = None;
        Ok(())
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params);
        if let Some(problem) = wanted.malformed() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        if wanted == self.settings {
            return Ok(Configure::applied());
        }
        let running = self.watcher.is_some();
        self.settings = wanted;
        if !running {
            return Ok(Configure::applied());
        }
        Ok(Configure::restart_required(
            "a WHEP session is negotiated once against one endpoint. Call plugin.reload \
             and the source negotiates again with the new settings.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.watcher.as_ref() {
            Some(watcher) => watcher.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            "stats" => Ok(json!({
                "endpoint": self.settings.redacted_endpoint(),
                "token": self.settings.has_token(),
                "health": self.watcher.as_ref().map(|w| w.health().detail).unwrap_or(None),
            })),
            other => Err(no_method("whip/whep", other)),
        }
    }
}

// ---------------------------------------------------------------------------

fn internal(message: String) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, message)
}

fn no_method(provide: &str, method: &str) -> RpcError {
    RpcError::new(
        codes::METHOD_NOT_FOUND,
        format!(
            "{provide} has no method '{method}'. It answers 'stats', and the standard \
             methods listed in docs/reference/plugin-protocol.md."
        ),
    )
}

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-whip is a GodwinMix plugin: the core starts it and talks JSON lines on \
             stdin and stderr.\nInstall it with `gmx plugin add plugins/whip`, then \
             `gmx output add away --type whip/output`.\nSee plugins/whip/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-whip could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    let outcome = match env.provide.as_str() {
        "whep" => runtime::run(
            &manifest,
            SourceHandler(WhepSource {
                settings: Settings::default(),
                reporter: None,
                watcher: None,
            }),
        ),
        _ => runtime::run(
            &manifest,
            OutputHandler(WhipOutput {
                settings: Settings::default(),
                reporter: None,
                sender: None,
            }),
        ),
    };
    if let Err(e) = outcome {
        eprintln!("whip/{} stopped: {e}", env.provide);
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::{Canvas, Transport};

    fn ready(params: Value) -> Ready {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.instance = "away".into();
        ready.provide = "output".into();
        ready.transport = Transport::Container;
        ready.params = params;
        ready
    }

    fn an_output() -> WhipOutput {
        WhipOutput { settings: Settings::default(), reporter: None, sender: None }
    }

    fn a_whep_source() -> WhepSource {
        WhepSource { settings: Settings::default(), reporter: None, watcher: None }
    }

    #[test]
    fn the_shipped_manifest_passes_the_validator_the_harness_runs() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let manifest = Manifest::load(root.join("gmx-plugin.toml"))
            .expect("the manifest beside this crate must load and validate");
        assert_eq!(manifest.plugin.name, "whip");
        assert!(manifest.provides.iter().any(|p| p.id == "output"));
        assert!(manifest.provides.iter().any(|p| p.id == "whep"));
    }

    #[test]
    fn an_output_with_no_endpoint_yet_comes_up_and_refuses_to_start() {
        let mut plugin = an_output();
        plugin
            .initialize(&ready(json!({})), Reporter::for_test())
            .expect("configure before start is legal, so a missing URL is not fatal here");
        let err = match plugin.start(&StartParams {
            canvas: Canvas::new(1280, 720, 30),
            transport: Transport::Container,
            media: "/tmp/gmx-whip-nothing".into(),
        }) {
            Ok(_) => panic!("an output with nowhere to send must not start"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("endpoint"), "{}", err.message);
    }

    #[test]
    fn a_running_output_asked_for_a_new_endpoint_says_restart_required() {
        let mut plugin = an_output();
        plugin.settings = Settings::from_params(&json!({"endpoint": "https://a/whip"}));
        // No sender, so this one applies outright.
        let applied = plugin
            .configure(json!({"endpoint": "https://b/whip"}))
            .expect("a stopped output takes a new endpoint");
        assert!(applied.applied);
        assert_eq!(plugin.settings.endpoint, "https://b/whip");
    }

    #[test]
    fn an_output_with_an_endpoint_but_no_fifo_names_the_field_that_is_missing() {
        let mut plugin = an_output();
        plugin.settings = Settings::from_params(&json!({"endpoint": "https://e/whip"}));
        let err = match plugin.start(&StartParams {
            canvas: Canvas::new(1280, 720, 30),
            transport: Transport::Container,
            media: String::new(),
        }) {
            Ok(_) => panic!("an output with no FIFO must not start"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("start.params.media"), "{}", err.message);
    }

    #[test]
    fn stats_never_return_the_token_itself() {
        let mut plugin = an_output();
        plugin.settings = Settings::from_params(&json!({
            "endpoint": "https://e/whip", "token": "very-secret-token"
        }));
        let answer = plugin.call("stats", Value::Null).expect("stats always answer");
        assert_eq!(answer["token"], true);
        assert!(!serde_json::to_string(&answer).unwrap().contains("very-secret-token"));
    }

    #[test]
    fn an_unknown_method_on_either_provide_names_what_is_there() {
        assert!(an_output()
            .call("teleport", Value::Null)
            .unwrap_err()
            .message
            .contains("stats"));
        assert!(a_whep_source()
            .call("teleport", Value::Null)
            .unwrap_err()
            .message
            .contains("stats"));
    }

    #[test]
    fn a_whep_source_with_a_bad_endpoint_is_refused_with_minus_32602() {
        let mut plugin = a_whep_source();
        let err = match plugin.initialize(&ready(json!({"endpoint": "srt://h:9000"})), Reporter::for_test()) {
            Ok(_) => panic!("an srt address is not a WHEP endpoint"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
    }

    #[test]
    fn health_before_start_is_degraded_on_both_provides() {
        assert_eq!(an_output().health().state, godwinmix_sdk::wire::HealthState::Degraded);
        assert_eq!(a_whep_source().health().state, godwinmix_sdk::wire::HealthState::Degraded);
    }
}
