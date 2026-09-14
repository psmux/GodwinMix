//! `gmx-director`, the process.
//!
//! Started by the core as a sidecar, or by hand with `--url` and `--token`.
//! See the head of `plugins/osc/src/main.rs` for why both exist.

use std::sync::Arc;

use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::ToolResult;
use gmx_director::service::{self, Log, Stderr, Wiring};
use gmx_director::{rules, Settings};
use serde_json::{json, Value};
use tokio::sync::watch;

const USAGE: &str = "\
gmx-director: an automatic director for GodwinMix.

Started by the core as a sidecar, or by hand:

  gmx-director --url http://127.0.0.1:8080 --token TOKEN --min-hold 6

  --url <url>         the mixer's control address (env GODWINMIX_URL)
  --token <token>     bearer token (env GODWINMIX_TOKEN)
  --interval <secs>   seconds between decisions, default 2
  --min-hold <secs>   seconds a shot is held, default 8
  --slow-look <secs>  move on after this long on one shot, default 45, 0 off
  --motion-delta <n>  how much more a source must move to earn a cut, default 0.15
  --source <id>       only take this source; repeatable
  --goal <text>       what the programme should show, for a model
  --llm <command>     a command to consult each cycle; omit for rules only
  --dry-run           decide and log, take nothing
  --version
  -h, --help

With no --llm it decides on rules alone and needs nothing but the mixer.
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return;
    }
    if argv.iter().any(|a| a == "--version") {
        println!("gmx-director {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let env = PluginEnv::from_env();
    if env.started_by_core() {
        run_as_sidecar(env);
    } else {
        run_standalone(argv);
    }
}

// ---------------------------------------------------------------------------
// Started by the core
// ---------------------------------------------------------------------------

struct CoreLog(Reporter);

impl Log for CoreLog {
    fn info(&self, message: &str) {
        self.0.info(message);
    }
    fn warn(&self, message: &str) {
        self.0.warn(message);
    }
}

struct Director {
    env: PluginEnv,
    settings: watch::Sender<Settings>,
    started: bool,
}

impl Service for Director {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        let _ = self.settings.send(Settings::from_value(&ready.params));
        if self.env.rpc.is_empty() {
            reporter.info(
                "no GMX_RPC in the environment, so there is no programme to direct. The plugin \
                 is up and will answer configure, health and explain.",
            );
            return Ok(InitializeResult { latency_ms: Some(0) });
        }
        if !self.started {
            self.started = true;
            spawn_worker(
                self.env.rpc.clone(),
                Some(self.env.token.clone()).filter(|t| !t.is_empty()),
                Arc::new(CoreLog(reporter)),
                self.settings.subscribe(),
            );
        }
        Ok(InitializeResult { latency_ms: Some(0) })
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        let _ = self.settings.send(Settings::from_value(&params));
        Ok(Configure::applied())
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> Result<ToolResult, RpcError> {
        match name {
            "explain" => explain(&self.settings.borrow(), arguments),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("gmx-director has no tool '{other}'. It has 'explain'."),
            )),
        }
    }
}

/// `explain`: what the director would do with a state, and why, without doing
/// it. An operator asks this before handing over a show; an agent asks it to
/// find out what the rules are without reading them.
fn explain(settings: &Settings, arguments: Value) -> Result<ToolResult, RpcError> {
    let state = arguments.get("state").cloned().ok_or_else(|| {
        RpcError::new(
            codes::INVALID_PARAMS,
            "explain wants a 'state': the agent.state document, or one shaped like it. \
             Example: explain {state: {program: \"cam1\", sources: [{id: \"cam1\", \
             state: \"live\"}, {id: \"cam2\", state: \"live\"}]}, held_secs: 30}.",
        )
    })?;
    let held = arguments
        .get("held_secs")
        .and_then(Value::as_f64)
        .unwrap_or(settings.min_hold_secs + 1.0);
    let view = service::view_from(&state, held);
    let decision = rules::decide(&view, settings);
    let (verb, source) = match &decision {
        rules::Decision::Hold(_) => ("hold", None),
        rules::Decision::Take { source, .. } => ("take", source.clone()),
    };
    Ok(ToolResult {
        content: json!([{ "type": "text", "text": format!("{verb}: {}", decision.why()) }]),
        structured_content: Some(json!({
            "decision": verb,
            "source": source,
            "why": decision.why(),
            "uses_a_model": settings.uses_a_model(),
        })),
        is_error: None,
    })
}

fn run_as_sidecar(env: PluginEnv) {
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml"))
        .expect("gmx-plugin.toml is beside the binary the core started");
    let (settings, _keep) = watch::channel(Settings::default());
    let plugin = Director {
        env,
        settings,
        started: false,
    };
    if let Err(error) = runtime::run(&manifest, ServiceHandler(plugin)) {
        eprintln!("gmx-director stopped: {error}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Started by hand
// ---------------------------------------------------------------------------

fn run_standalone(argv: Vec<String>) {
    let mut url = std::env::var("GODWINMIX_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".into());
    let mut token = std::env::var("GODWINMIX_TOKEN").ok().filter(|t| !t.is_empty());
    let mut object = json!({});
    let mut sources: Vec<Value> = Vec::new();

    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--url" => url = value(),
            "--token" => token = Some(value()).filter(|t| !t.is_empty()),
            "--interval" => object["interval_secs"] = number(&value()),
            "--min-hold" => object["min_hold_secs"] = number(&value()),
            "--slow-look" => object["slow_look_secs"] = number(&value()),
            "--motion-delta" => object["motion_delta"] = number(&value()),
            "--goal" => object["goal"] = Value::String(value()),
            "--llm" => object["llm"] = Value::String(value()),
            "--source" => sources.push(Value::String(value())),
            "--dry-run" => object["dry_run"] = Value::Bool(true),
            other => {
                eprintln!("gmx-director: '{other}' is not a flag this program takes.\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if !sources.is_empty() {
        object["sources"] = Value::Array(sources);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let (tx, rx) = watch::channel(Settings::from_value(&object));
    let _keep = tx;
    runtime.block_on(service::run(
        Wiring {
            url,
            token,
            log: Arc::new(Stderr),
        },
        rx,
    ));
}

fn number(text: &str) -> Value {
    match text.parse::<f64>() {
        Ok(n) => json!(n),
        Err(_) => {
            eprintln!("gmx-director: '{text}' is not a number.");
            std::process::exit(2);
        }
    }
}

fn spawn_worker(
    url: String,
    token: Option<String>,
    log: Arc<dyn Log>,
    settings: watch::Receiver<Settings>,
) {
    std::thread::Builder::new()
        .name("gmx-director".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(error) => {
                    log.warn(&format!("could not start the worker runtime: {error}"));
                    return;
                }
            };
            runtime.block_on(service::run(Wiring { url, token, log }, settings));
        })
        .expect("a worker thread");
}
