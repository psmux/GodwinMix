//! `gmx-ograf`, the process.
//!
//! Started by the core as a sidecar, or by hand to look at a graphic in a
//! browser before it goes anywhere near a programme. See the head of
//! `plugins/osc/src/main.rs` for why both exist.

use gmx_ograf::host::{self, DEFAULT_PORT};
use gmx_ograf::state::Host;
use godwinmix_sdk::prelude::*;
use godwinmix_sdk::wire::ToolResult;
use serde_json::{json, Map, Value};

const USAGE: &str = "\
gmx-ograf: the OGraf graphics host for GodwinMix.

Started by the core as a sidecar, or by hand to look at a graphic:

  gmx-ograf --serve --port 7841
  open http://127.0.0.1:7841/

  --serve          serve the graphics and stay up
  --port <n>       the port to ask for, default 7841, 0 for any free one
  --root <dir>     the plugin directory to read graphics from, default this one
  --version
  -h, --help

With --serve it prints the address and every graphic it found, so a template
can be opened in a browser and reloaded while it is written.
";

fn main() {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        print!("{USAGE}");
        return;
    }
    if argv.iter().any(|a| a == "--version") {
        println!("gmx-ograf {}", env!("CARGO_PKG_VERSION"));
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

struct Ograf {
    root: std::path::PathBuf,
    host: Host,
    runtime: tokio::runtime::Runtime,
    base: Option<String>,
}

impl Service for Ograf {
    fn initialize(
        &mut self,
        ready: &Ready,
        reporter: Reporter,
    ) -> Result<InitializeResult, RpcError> {
        let want = port_from(&ready.params);
        let serving = self
            .runtime
            .block_on(host::serve(self.root.clone(), self.host.clone(), want))
            .map_err(|e| {
                RpcError::new(
                    codes::INTERNAL_ERROR,
                    format!(
                        "the graphics host could not listen on 127.0.0.1: {e}. \
                         Set `port` in the plugin's settings to a free one, or 0 for any."
                    ),
                )
            })?;
        let found: Vec<String> = gmx_ograf::catalogue::all(&self.root)
            .iter()
            .map(|g| g.type_id())
            .collect();
        let serving_what = match found.len() {
            0 => "no graphics yet".to_string(),
            1 => found[0].clone(),
            n => format!("{n} graphics: {}", found.join(", ")),
        };
        reporter.info(&format!(
            "the graphics host is serving {serving_what} on {base}",
            base = serving.base
        ) as &str);
        self.base = Some(serving.base);
        Ok(InitializeResult {
            latency_ms: Some(0),
        })
    }

    fn configure(&mut self, params: Value) -> Result<Configure, RpcError> {
        // The port is the only setting, and a port is not changed under a
        // browser that is already pointed at it.
        match port_from(&params) == self.port() {
            true => Ok(Configure::applied()),
            false => Ok(Configure::restart_required(
                "the graphics host takes a new port by being started again. \
                 Every graphic on air is pointed at the old one.",
            )),
        }
    }

    fn tool_call(&mut self, name: &str, arguments: Value) -> Result<ToolResult, RpcError> {
        match name {
            "graphic" => self.graphic(arguments),
            other => Err(RpcError::new(
                codes::METHOD_NOT_FOUND,
                format!("gmx-ograf has no tool '{other}'. It has 'graphic'."),
            )),
        }
    }
}

impl Ograf {
    fn port(&self) -> u16 {
        self.base
            .as_deref()
            .and_then(|b| b.rsplit(':').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT)
    }

    /// The one tool: where the host is, what it is driving, and the four OGraf
    /// actions.
    fn graphic(&mut self, arguments: Value) -> Result<ToolResult, RpcError> {
        let action = arguments
            .get("action")
            .and_then(Value::as_str)
            .unwrap_or("status");
        let instance = arguments
            .get("instance")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let values: Map<String, Value> = arguments
            .get("values")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        let base = self.base.clone().unwrap_or_default();
        let structured = match action {
            "where" => json!({
                "base": base,
                "graphics": gmx_ograf::catalogue::all(&self.root)
                    .iter()
                    .map(|g| json!({ "graphic": g.type_id(), "page": format!("{base}/graphic/{}", g.type_id()) }))
                    .collect::<Vec<_>>(),
            }),
            "status" if instance.is_empty() => json!({
                "base": base,
                "instances": self.host.all().iter().map(|(id, i)| i.as_json(id)).collect::<Vec<_>>(),
            }),
            "status" => self.state(instance)?.as_json(instance),
            "load" => {
                let graphic = arguments
                    .get("graphic")
                    .and_then(Value::as_str)
                    .ok_or_else(|| {
                        RpcError::new(
                            codes::INVALID_PARAMS,
                            "load wants a `graphic`, the plugin qualified id of the template, \
                         for example {instance: \"graphic-lower-third-9f2c41ab\", \
                         graphic: \"ograf/lower-third\", action: \"load\"}.",
                        )
                    })?;
                self.named(instance)?;
                self.host.load(instance, graphic, &values).as_json(instance)
            }
            "update" => {
                self.named(instance)?;
                self.state(instance)?;
                self.host
                    .update(instance, &values)
                    .map(|s| s.as_json(instance))
                    .unwrap_or_default()
            }
            "play" => {
                self.named(instance)?;
                self.state(instance)?;
                let step = arguments
                    .get("step")
                    .and_then(Value::as_u64)
                    .map(|s| s as u32);
                self.host
                    .play(instance, step)
                    .map(|s| s.as_json(instance))
                    .unwrap_or_default()
            }
            "stop" => {
                self.named(instance)?;
                self.state(instance)?;
                self.host
                    .stop(instance)
                    .map(|s| s.as_json(instance))
                    .unwrap_or_default()
            }
            "forget" => {
                self.named(instance)?;
                json!({ "forgotten": self.host.forget(instance), "instance": instance })
            }
            other => {
                return Err(RpcError::new(
                    codes::INVALID_PARAMS,
                    format!(
                        "`{other}` is not an action. It takes: where, status, load, \
                         update, play, stop, forget."
                    ),
                ))
            }
        };
        Ok(ToolResult {
            content: json!([{ "type": "text", "text": summarise(action, instance, &structured) }]),
            structured_content: Some(structured),
            is_error: None,
        })
    }

    /// Refuse an action with no instance, naming the ones that are loaded.
    fn named(&self, instance: &str) -> Result<(), RpcError> {
        if !instance.is_empty() {
            return Ok(());
        }
        Err(RpcError::new(
            codes::INVALID_PARAMS,
            format!(
                "this action needs an `instance`: the source id of the graphic placement, \
                 which scene.get gives you. Loaded now: {}.",
                self.loaded()
            ),
        ))
    }

    fn state(&self, instance: &str) -> Result<gmx_ograf::state::Instance, RpcError> {
        self.host.get(instance).ok_or_else(|| {
            RpcError::new(
                codes::INVALID_PARAMS,
                format!(
                    "nothing is loaded into `{instance}`. Load it first, or put the graphic \
                     on a scene and let the core do it. Loaded now: {}.",
                    self.loaded()
                ),
            )
        })
    }

    fn loaded(&self) -> String {
        let all = self.host.all();
        match all.is_empty() {
            true => "nothing".into(),
            false => all
                .iter()
                .map(|(id, _)| id.clone())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// One line a person can read, beside the structured answer a model reads.
fn summarise(action: &str, instance: &str, body: &Value) -> String {
    match action {
        "where" => format!(
            "the graphics host is on {}",
            body["base"].as_str().unwrap_or("?")
        ),
        "status" if instance.is_empty() => format!(
            "{} graphics loaded",
            body["instances"].as_array().map(Vec::len).unwrap_or(0)
        ),
        "play" => format!("{instance} is playing step {}", body["step"]),
        "stop" => format!("{instance} is off"),
        _ => format!("{action} {instance}"),
    }
}

fn port_from(params: &Value) -> u16 {
    params
        .get("port")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(DEFAULT_PORT)
}

fn run_as_sidecar(env: PluginEnv) {
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml"))
        .expect("gmx-plugin.toml is beside the binary the core started");
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a tokio runtime");
    let plugin = Ograf {
        root: env.root.clone(),
        host: Host::new(),
        runtime,
        base: None,
    };
    if let Err(error) = runtime::run(&manifest, ServiceHandler(plugin)) {
        eprintln!("gmx-ograf stopped: {error}");
        std::process::exit(1);
    }
}

// ---------------------------------------------------------------------------
// Started by hand
// ---------------------------------------------------------------------------

fn run_standalone(argv: Vec<String>) {
    let mut port = DEFAULT_PORT;
    let mut root = std::env::current_dir().unwrap_or_default();
    let mut serve = false;
    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--serve" => serve = true,
            "--port" => {
                port = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(DEFAULT_PORT)
            }
            "--root" => root = it.next().map(std::path::PathBuf::from).unwrap_or(root),
            other => {
                eprintln!("gmx-ograf: '{other}' is not a flag this program takes.\n\n{USAGE}");
                std::process::exit(2);
            }
        }
    }
    if !serve {
        print!("{USAGE}");
        return;
    }
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
        .expect("a tokio runtime");
    runtime.block_on(async move {
        let host = Host::new();
        let serving = match host::serve(root.clone(), host, port).await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("gmx-ograf: could not listen on 127.0.0.1:{port}: {e}");
                std::process::exit(1);
            }
        };
        println!("serving on {}", serving.base);
        for g in gmx_ograf::catalogue::all(&root) {
            println!("  {}/graphic/{}", serving.base, g.type_id());
        }
        if gmx_ograf::catalogue::all(&root).is_empty() {
            println!("  no graphics found under {}", root.display());
        }
        std::future::pending::<()>().await;
    });
}
