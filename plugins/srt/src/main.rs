//! `srt/source`: receive SRT, in caller or listener mode.
//!
//! SRT is how a stream crosses a link that loses packets: it retransmits inside
//! a latency budget you choose, so a 200 ms budget rides out a lot of a bad
//! hotel wifi. This plugin receives one. It does not send: `srt/output` is
//! built into the core at tier 0 (`crates/godwinmix-core/src/plugin/outputs/
//! srt.rs`) and stays there, because the programme is already encoded on the
//! tee and a sidecar would only add a copy.
//!
//! What crosses to the core is the MPEG-TS exactly as it arrived. The core's
//! container transport is `fdsrc ! decodebin`, so the decode happens once, in
//! the core, on its hardware aware path.

mod source;
mod uri;

use godwinmix_sdk::prelude::*;
use serde_json::{json, Value};

use source::Receiver;
use uri::Settings;

struct SrtSource {
    settings: Settings,
    reporter: Option<Reporter>,
    receiver: Option<Receiver>,
}

impl SrtSource {
    fn new() -> SrtSource {
        SrtSource {
            settings: Settings::default(),
            reporter: None,
            receiver: None,
        }
    }
}

impl Source for SrtSource {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        self.settings = Settings::from_params(&ready.params);
        if let Some(problem) = self.settings.problem() {
            return Err(RpcError::new(codes::INVALID_PARAMS, problem));
        }
        gmx_netkit::init().map_err(internal)?;
        gmx_netkit::elements::require(&["srtsrc"]).map_err(internal)?;
        reporter.info(format!(
            "srt/source '{}' will receive {}",
            ready.instance,
            self.settings.redacted()
        ));
        self.reporter = Some(reporter);
        // The receive buffer is the latency this source adds, and it is the
        // number the operator chose, so the aligner is told exactly that.
        Ok(InitializeResult {
            latency_ms: Some(self.settings.latency_ms),
        })
    }

    fn start(&mut self, _params: &StartParams) -> Result<StartResult, RpcError> {
        let receiver = Receiver::start(&self.settings, self.reporter.clone()).map_err(internal)?;
        self.receiver = Some(receiver);
        Ok(StartResult {
            latency_ms: Some(self.settings.latency_ms),
        })
    }

    fn stop(&mut self) -> Result<(), RpcError> {
        self.receiver = None;
        Ok(())
    }

    /// An SRT connection is made once, from an address, with a key. Changing
    /// any of that means dialling again, so `configure` says so rather than
    /// pretending a live socket can be edited underneath the stream.
    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let wanted = Settings::from_params(&params);
        if let Some(problem) = wanted.problem() {
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
            "an SRT connection is made once, from one address with one key. \
             Call plugin.reload and the source dials again with the new settings.",
        ))
    }

    fn health(&mut self) -> Health {
        match self.receiver.as_ref() {
            Some(receiver) => receiver.health(),
            None => Health::degraded("not started yet"),
        }
    }

    fn call(&mut self, method: &str, _params: Value) -> Result<Value, RpcError> {
        match method {
            // The same call the built in srt/output answers, so a client
            // reading link quality does not care which end it is asking.
            "stats" => Ok(json!({
                "address": self.settings.redacted(),
                "stats": self.receiver.as_ref().map(|r| r.stats()).unwrap_or(Value::Null),
            })),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!(
                    "srt/source has no method '{other}'. It answers 'stats', and the \
                     standard source methods listed in docs/reference/plugin-protocol.md."
                ),
            )),
        }
    }
}

fn internal(message: String) -> RpcError {
    RpcError::new(codes::INTERNAL_ERROR, message)
}

fn main() {
    let env = PluginEnv::from_env();
    if !env.started_by_core() {
        eprintln!(
            "gmx-srt is a GodwinMix plugin: the core starts it and talks JSON lines on \
             stdin and stderr.\nInstall it with `gmx plugin add plugins/srt`, then \
             `gmx source add guest --type srt/source`.\nSee plugins/srt/README.md."
        );
        std::process::exit(2);
    }
    let manifest = match Manifest::load(env.root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("gmx-srt could not read its own gmx-plugin.toml: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = runtime::run(&manifest, SourceHandler(SrtSource::new())) {
        eprintln!("srt/source stopped: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_sdk::wire::{Canvas, Transport};

    fn ready(params: Value) -> Ready {
        let mut ready = Ready::for_test(Canvas::new(1280, 720, 30));
        ready.instance = "guest".into();
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
        assert_eq!(manifest.plugin.name, "srt");
        assert!(manifest.provides.iter().any(|p| p.id == "source"));
    }

    #[test]
    fn nonsense_settings_are_refused_at_initialize_with_minus_32602() {
        let mut plugin = SrtSource::new();
        let err = match plugin.initialize(&ready(json!({"mode": "sideways"})), Reporter::for_test()) {
            Ok(_) => panic!("a mode that does not exist must be refused"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert!(err.message.contains("rendezvous"), "{}", err.message);
    }

    #[test]
    fn the_same_settings_twice_apply_without_a_restart() {
        let mut plugin = SrtSource::new();
        plugin.settings = Settings::from_params(&json!({"host": "h", "port": 9000}));
        let again = plugin
            .configure(json!({"host": "h", "port": 9000}))
            .expect("the same settings are always appliable");
        assert!(again.applied);
    }

    #[test]
    fn a_new_address_while_stopped_is_applied_rather_than_refused() {
        let mut plugin = SrtSource::new();
        let moved = plugin
            .configure(json!({"host": "other", "port": 4200}))
            .expect("a stopped source takes a new address");
        assert!(moved.applied);
        assert_eq!(plugin.settings.host, "other");
    }

    #[test]
    fn a_bad_address_in_configure_is_refused_and_the_old_one_is_kept() {
        let mut plugin = SrtSource::new();
        plugin.settings = Settings::from_params(&json!({"host": "good"}));
        let err = match plugin.configure(json!({"uri": "http://nope"})) {
            Ok(_) => panic!("an http address is not an srt address"),
            Err(e) => e,
        };
        assert_eq!(err.code, codes::INVALID_PARAMS);
        assert_eq!(plugin.settings.host, "good");
    }

    #[test]
    fn an_unknown_method_names_what_this_plugin_does_answer() {
        let mut plugin = SrtSource::new();
        let err = plugin.call("teleport", Value::Null).unwrap_err();
        assert_eq!(err.code, codes::METHOD_NOT_FOUND);
        assert!(err.message.contains("stats"), "{}", err.message);
    }

    #[test]
    fn stats_before_start_answer_with_the_address_and_no_numbers() {
        let mut plugin = SrtSource::new();
        plugin.settings = Settings::from_params(&json!({"host": "h", "passphrase": "abcdefghij"}));
        let answer = plugin.call("stats", Value::Null).expect("stats always answer");
        assert!(!answer["address"].as_str().unwrap().contains("abcdefghij"));
        assert!(answer["stats"].is_null());
    }

    #[test]
    fn health_before_start_is_degraded_not_failing() {
        let mut plugin = SrtSource::new();
        assert_eq!(plugin.health().state, godwinmix_sdk::wire::HealthState::Degraded);
    }

}
