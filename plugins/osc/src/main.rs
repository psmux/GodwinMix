//! `gmx-osc`, the process.
//!
//! It starts in one of two ways and behaves the same in both.
//!
//! * The core starts it, as a sidecar. `GMX_INSTANCE` and friends are set, the
//!   JSON lines protocol runs on stdin and stderr, `configure` arrives there,
//!   and the address of the core's `/rpc` arrives in `GMX_RPC`.
//! * A person starts it, with `--url` and `--token`. There is no stdio
//!   protocol, settings come off the command line, and logs go to stderr.
//!
//! The second mode exists because the mixer does not yet instantiate `service`
//! plugins itself: `SidecarService` is built in `godwinmix-host` and nothing
//! constructs it. Until it does, `gmx-osc --url ... --token ...` is how you run
//! this against a live core, and it is also how the live test in the README
//! runs. Once the core instantiates services, neither this file nor the
//! manifest changes.

use std::sync::Arc;

use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::ToolResult;
use gmx_osc::service::{self, Log, Stderr, Wiring};
use gmx_osc::Settings;
use serde_json::{json, Value};
use tokio::sync::watch;

const USAGE: &str = "\
gmx-osc: an OSC bridge for GodwinMix.

Started by the core as a sidecar, or by hand:

  gmx-osc --url http://127.0.0.1:8080 --token TOKEN [options]

  --url <url>        the mixer's control address (env GODWINMIX_URL)
  --token <token>    bearer token (env GODWINMIX_TOKEN)
  --listen <addr>    where to receive OSC, default 0.0.0.0:9000
  --send-to <addr>   where to send tally and programme; repeatable
  --prefix <path>    address prefix on everything sent out, default /gmx
  --allow <addr>     only act on packets from this address; repeatable
  --version
  -h, --help

With no arguments and no GMX_INSTANCE in the environment it prints this.
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return;
    }
    if argv.iter().any(|a| a == "--version") {
        println!("gmx-osc {}", env!("CARGO_PKG_VERSION"));
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

/// The core's log, reached through the SDK's `Reporter`.
struct CoreLog(Reporter);

impl Log for CoreLog {
    fn info(&self, message: &str) {
        self.0.info(message);
    }
    fn warn(&self, message: &str) {
        self.0.warn(message);
    }
}

/// The plugin as the SDK's runtime sees it.
///
/// Everything it does is done by the worker; this half exists to answer
/// `initialize`, `configure`, `health` and `tool.call` promptly, which is the
/// rule the two thread runtime is built to keep.
struct Osc {
    env: PluginEnv,
    settings: watch::Sender<Settings>,
    started: bool,
}

impl Service for Osc {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        let settings = Settings::from_value(&ready.params);
        let _ = self.settings.send(settings.clone());
        if self.env.rpc.is_empty() {
            // `gmx plugin test` spawns a plugin with no core behind it. Say so
            // once and stay up: the harness checks the handshake and
            // `configure`, and neither needs a socket.
            reporter.info(
                "no GMX_RPC in the environment, so there is no core to call. The plugin is up \
                 and will answer configure and health; nothing is listening on the OSC port.",
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
        // Rebinding the socket is what the worker does when the channel
        // changes, so nothing here needs a restart.
        Ok(Configure::applied())
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> Result<ToolResult, RpcError> {
        match name {
            "send_osc" => tool_send_osc(&self.settings.borrow(), arguments),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("gmx-osc has no tool '{other}'. It has 'send_osc'."),
            )),
        }
    }
}

/// `send_osc`: put one message on the wire, so an operator can prove the path
/// to a lamp or a tablet without leaving the mixer.
fn tool_send_osc(settings: &Settings, arguments: Value) -> Result<ToolResult, RpcError> {
    let address = arguments
        .get("address")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            RpcError::new(
                codes::INVALID_PARAMS,
                "send_osc wants an 'address', for example \"/gmx/tally/cam1\".",
            )
        })?;
    let args: Vec<gmx_osc::Arg> = arguments
        .get("args")
        .and_then(Value::as_array)
        .map(|list| list.iter().map(json_to_arg).collect())
        .unwrap_or_default();
    let targets: Vec<String> = match arguments.get("to").and_then(Value::as_str) {
        Some(one) => vec![one.to_string()],
        None => settings.send_to.clone(),
    };
    if targets.is_empty() {
        return Err(RpcError::new(
            codes::INVALID_PARAMS,
            "there is nowhere to send to: 'send_to' is empty in the settings and the call gave \
             no 'to'. Set one of them.",
        ));
    }
    let bytes = gmx_osc::Message::new(address, args).encode();
    let socket = std::net::UdpSocket::bind("0.0.0.0:0")
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, format!("no socket: {e}")))?;
    let mut sent = Vec::new();
    for target in &targets {
        match socket.send_to(&bytes, target) {
            Ok(_) => sent.push(target.clone()),
            Err(error) => {
                return Err(RpcError::new(
                    codes::INTERNAL_ERROR,
                    format!("could not send to {target}: {error}"),
                ))
            }
        }
    }
    Ok(text_result(format!(
        "sent {} bytes to {}: {address}",
        bytes.len(),
        sent.join(", ")
    )))
}

/// A tool answer in MCP's shape: one block of text.
fn text_result(text: String) -> ToolResult {
    ToolResult {
        content: json!([{"type": "text", "text": text}]),
        structured_content: None,
        is_error: None,
    }
}

fn json_to_arg(value: &Value) -> gmx_osc::Arg {
    match value {
        Value::String(s) => gmx_osc::Arg::Str(s.clone()),
        Value::Bool(b) => gmx_osc::Arg::Bool(*b),
        Value::Number(n) if n.is_i64() => gmx_osc::Arg::Int(n.as_i64().unwrap_or(0) as i32),
        Value::Number(n) => gmx_osc::Arg::Float(n.as_f64().unwrap_or(0.0) as f32),
        other => gmx_osc::Arg::Str(other.to_string()),
    }
}

fn run_as_sidecar(env: PluginEnv) {
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml"))
        .expect("gmx-plugin.toml is beside the binary the core started");
    let (settings, _keep) = watch::channel(Settings::default());
    let plugin = Osc {
        env,
        settings,
        started: false,
    };
    if let Err(error) = runtime::run(&manifest, ServiceHandler(plugin)) {
        eprintln!("gmx-osc stopped: {error}");
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
    let mut send_to: Vec<Value> = Vec::new();
    let mut allow: Vec<Value> = Vec::new();

    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--url" => url = value(),
            "--token" => token = Some(value()).filter(|t| !t.is_empty()),
            "--listen" => object["listen"] = Value::String(value()),
            "--prefix" => object["prefix"] = Value::String(value()),
            "--send-to" => send_to.push(Value::String(value())),
            "--allow" => allow.push(Value::String(value())),
            other => {
                eprintln!("gmx-osc: '{other}' is not a flag this program takes.\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if !send_to.is_empty() {
        object["send_to"] = Value::Array(send_to);
    }
    if !allow.is_empty() {
        object["allow_from"] = Value::Array(allow);
    }

    let settings = Settings::from_value(&object);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let (tx, rx) = watch::channel(settings);
    // Held so the channel stays open for as long as the process runs.
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

/// The worker thread a sidecar runs the async half on.
///
/// The SDK's main loop owns this thread and is deliberately not async; the
/// control work is. One thread with a current thread runtime is the whole
/// bridge between them, and it costs about 2 MB of stack that is never on a
/// media path.
fn spawn_worker(
    url: String,
    token: Option<String>,
    log: Arc<dyn Log>,
    settings: watch::Receiver<Settings>,
) {
    std::thread::Builder::new()
        .name("gmx-osc".into())
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
