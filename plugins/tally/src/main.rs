//! `gmx-tally`, the process.
//!
//! Started by the core as a sidecar, or by hand with `--url` and `--token`.
//! See the head of `plugins/osc/src/main.rs` for why both exist; the two files
//! are deliberately the same shape, because a plugin author copying one of
//! them should not have to work out which parts were accidental.

use std::sync::Arc;

use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::ToolResult;
use gmx_tally::sender::Sender;
use gmx_tally::service::{self, Log, Stderr, Wiring};
use gmx_tally::{Display, Lamp, Settings};
use serde_json::{json, Value};
use tokio::sync::watch;

const USAGE: &str = "\
gmx-tally: TSL UMD v5 tally lamps for GodwinMix.

Started by the core as a sidecar, or by hand:

  gmx-tally --url http://127.0.0.1:8080 --token TOKEN --to 10.0.0.30:8900 \\
            --lamp cam1:0:'CAM 1' --lamp cam2:1:'CAM 2'

  --url <url>        the mixer's control address (env GODWINMIX_URL)
  --token <token>    bearer token (env GODWINMIX_TOKEN)
  --to <addr>        where the lamps are, default 127.0.0.1:8900
  --tcp              send over TCP instead of UDP
  --screen <n>       the display group, default 0
  --lamp <spec>      source:index:label; repeatable. index and label optional
  --program <col>    red, green or amber; default red
  --preview <col>    default green
  --refresh <secs>   resend every lamp this often, default 10, 0 to turn off
  --version
  -h, --help
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return;
    }
    if argv.iter().any(|a| a == "--version") {
        println!("gmx-tally {}", env!("CARGO_PKG_VERSION"));
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

struct Tally {
    env: PluginEnv,
    settings: watch::Sender<Settings>,
    started: bool,
}

impl Service for Tally {
    fn initialize(&mut self, ready: &Ready, reporter: Reporter) -> Result<InitializeResult, RpcError> {
        let _ = self.settings.send(Settings::from_value(&ready.params));
        if self.env.rpc.is_empty() {
            reporter.info(
                "no GMX_RPC in the environment, so there is no core to watch. The plugin is up \
                 and will answer configure, health and test_lamp; no lamp is being driven.",
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
            "test_lamp" => test_lamp(&self.settings.borrow().clone(), arguments),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("gmx-tally has no tool '{other}'. It has 'test_lamp'."),
            )),
        }
    }
}

/// `test_lamp`: light one lamp for a moment so an engineer on a ladder can see
/// which index is which without anybody taking a source.
fn test_lamp(settings: &Settings, arguments: Value) -> Result<ToolResult, RpcError> {
    let index = arguments
        .get("index")
        .and_then(Value::as_u64)
        .map(|n| n.min(u16::MAX as u64) as u16);
    let source = arguments.get("source").and_then(Value::as_str);
    let (index, label) = match (index, source) {
        (Some(index), _) => (index, source.unwrap_or("TEST").to_string()),
        (None, Some(source)) => {
            let lamp = settings.lamp_for(source).ok_or_else(|| {
                let known: Vec<&str> = settings.lamps.iter().map(|l| l.source.as_str()).collect();
                RpcError::new(
                    codes::INVALID_PARAMS,
                    format!(
                        "no lamp watches '{source}'. The lamps are for: {}. Add one to `lamps` \
                         in the settings, or give an `index` instead.",
                        if known.is_empty() { "nothing yet".into() } else { known.join(", ") }
                    ),
                )
            })?;
            (lamp.index, lamp.label.clone())
        }
        (None, None) => {
            return Err(RpcError::new(
                codes::INVALID_PARAMS,
                "test_lamp wants an `index` or a `source`. Example: test_lamp {index: 0}.",
            ))
        }
    };
    let colour = arguments
        .get("colour")
        .and_then(Value::as_str)
        .and_then(Lamp::parse)
        .unwrap_or(Lamp::Red);
    let label = arguments
        .get("label")
        .and_then(Value::as_str)
        .filter(|l| !l.is_empty())
        .map(str::to_string)
        .unwrap_or(label);
    let hold_ms = arguments
        .get("hold_ms")
        .and_then(Value::as_u64)
        .unwrap_or(2_000)
        .clamp(100, 30_000);

    let lit = Display {
        index,
        right: colour,
        text: colour,
        left: Lamp::Off,
        brightness: settings.brightness,
        label: label.clone(),
    };
    let dark = Display {
        right: Lamp::Off,
        text: Lamp::Off,
        ..lit.clone()
    };

    // A short blocking runtime rather than the worker's: a tool call answers
    // when the lamp has been put back, and the worker must keep serving tally
    // while this runs.
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| RpcError::new(codes::INTERNAL_ERROR, format!("no runtime: {e}")))?;
    let where_to = settings.address.clone();
    let settings = settings.clone();
    runtime.block_on(async move {
        let mut sender = Sender::open(settings.protocol, &settings.address)
            .await
            .map_err(|e| {
                RpcError::new(
                    codes::INTERNAL_ERROR,
                    format!("cannot reach {}: {e}", settings.address),
                )
            })?;
        sender
            .send(&gmx_tally::tsl::encode(settings.screen, &lit, settings.unicode))
            .await
            .map_err(|e| {
                RpcError::new(
                    codes::INTERNAL_ERROR,
                    format!("could not light lamp {index} on {}: {e}", settings.address),
                )
            })?;
        tokio::time::sleep(std::time::Duration::from_millis(hold_ms)).await;
        let _ = sender
            .send(&gmx_tally::tsl::encode(settings.screen, &dark, settings.unicode))
            .await;
        Ok::<(), RpcError>(())
    })?;

    Ok(text_result(format!(
        "lit lamp {index} ({label}) {} for {hold_ms} ms on {}, then put it out. The next tally \
         change or the refresh timer restores what it should be showing.",
        colour.name(),
        where_to
    )))
}

fn text_result(text: String) -> ToolResult {
    ToolResult {
        content: json!([{"type": "text", "text": text}]),
        structured_content: None,
        is_error: None,
    }
}

fn run_as_sidecar(env: PluginEnv) {
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml"))
        .expect("gmx-plugin.toml is beside the binary the core started");
    let (settings, _keep) = watch::channel(Settings::default());
    let plugin = Tally {
        env,
        settings,
        started: false,
    };
    if let Err(error) = runtime::run(&manifest, ServiceHandler(plugin)) {
        eprintln!("gmx-tally stopped: {error}");
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
    let mut lamps: Vec<Value> = Vec::new();

    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--url" => url = value(),
            "--token" => token = Some(value()).filter(|t| !t.is_empty()),
            "--to" => object["address"] = Value::String(value()),
            "--tcp" => object["protocol"] = Value::String("tcp".into()),
            "--screen" => object["screen"] = json!(value().parse::<u64>().unwrap_or(0)),
            "--program" => object["program_colour"] = Value::String(value()),
            "--preview" => object["preview_colour"] = Value::String(value()),
            "--refresh" => object["refresh_secs"] = json!(value().parse::<u64>().unwrap_or(10)),
            "--lamp" => lamps.push(lamp_spec(&value(), lamps.len())),
            other => {
                eprintln!("gmx-tally: '{other}' is not a flag this program takes.\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if !lamps.is_empty() {
        object["lamps"] = Value::Array(lamps);
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

/// `source`, `source:index` or `source:index:label`.
fn lamp_spec(text: &str, position: usize) -> Value {
    let mut pieces = text.splitn(3, ':');
    let source = pieces.next().unwrap_or("").to_string();
    let index = pieces
        .next()
        .and_then(|n| n.parse::<u64>().ok())
        .unwrap_or(position as u64);
    let label = pieces.next().unwrap_or(&source).to_string();
    json!({"source": source, "index": index, "label": label})
}

fn spawn_worker(
    url: String,
    token: Option<String>,
    log: Arc<dyn Log>,
    settings: watch::Receiver<Settings>,
) {
    std::thread::Builder::new()
        .name("gmx-tally".into())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lamp_spec_reads_all_three_forms() {
        assert_eq!(lamp_spec("cam1", 4), json!({"source": "cam1", "index": 4, "label": "cam1"}));
        assert_eq!(lamp_spec("cam1:2", 4), json!({"source": "cam1", "index": 2, "label": "cam1"}));
        assert_eq!(
            lamp_spec("cam1:2:CAM 1", 4),
            json!({"source": "cam1", "index": 2, "label": "CAM 1"})
        );
    }

    #[test]
    fn a_label_with_a_colon_in_it_survives() {
        assert_eq!(
            lamp_spec("cam1:0:Stage: wide", 0),
            json!({"source": "cam1", "index": 0, "label": "Stage: wide"})
        );
    }
}
