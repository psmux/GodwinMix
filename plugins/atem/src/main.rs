//! `gmx-atem`, the process.
//!
//! Started by the core as a sidecar, or by hand to drive switchers from a
//! terminal with one tool call per line. See the head of
//! `plugins/osc/src/main.rs` for why both exist.

use std::sync::Arc;

use gmx_atem::service::{repl, AtemService};
use gmx_atem::Stderr;
use godwinmix_sdk::prelude::*;
use serde_json::json;

const USAGE: &str = "\
gmx-atem: Blackmagic ATEM switchers for GodwinMix.

Started by the core as a sidecar, or by hand, reading one tool call per line:

  gmx-atem --switcher atem-main=192.0.2.10 --switcher atem-overflow=192.0.2.11
  list {}
  preview {\"switcher\": \"atem-main\", \"input\": 2}
  auto {\"switcher\": \"atem-main\"}

  --switcher <id=address>  a switcher to connect; repeatable. The address may
                           carry :port when it is not 9910
  --url <url>              the mixer's address, for follow and lead (env GODWINMIX_URL)
  --token <token>          bearer token (env GODWINMIX_TOKEN)
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
        println!("gmx-atem {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let env = PluginEnv::from_env();
    if env.started_by_core() {
        let manifest = Manifest::load(env.root.join("gmx-plugin.toml")).expect("gmx-plugin.toml is beside the binary the core started");
        if let Err(error) = runtime::run(&manifest, ServiceHandler(AtemService::new(env))) {
            eprintln!("gmx-atem stopped: {error}");
            std::process::exit(1);
        }
    } else {
        run_standalone(env, argv);
    }
}

fn run_standalone(env: PluginEnv, argv: Vec<String>) {
    let mut url = std::env::var("GODWINMIX_URL").ok().filter(|u| !u.is_empty());
    let mut token = std::env::var("GODWINMIX_TOKEN").ok().filter(|t| !t.is_empty());
    let mut switchers = Vec::new();
    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--url" => url = Some(value()),
            "--token" => token = Some(value()),
            "--switcher" => {
                let spec = value();
                let (id, address) = spec.split_once('=').unwrap_or_else(|| fail(&format!("'{spec}' is not id=address, such as atem-main=192.0.2.10")));
                switchers.push(json!({"id": id, "address": address}));
            }
            other => fail(&format!("'{other}' is not a flag this program takes.\n\n{USAGE}")),
        }
    }
    let mut service = AtemService::new(env);
    let problems = service.desk.lock().unwrap().configure(gmx_atem::Settings::from_value(&json!({"switchers": switchers})));
    for p in problems {
        eprintln!("gmx-atem: {p}");
    }
    if let Some(url) = url {
        service.start_sync(url, token, Arc::new(Stderr));
    }
    eprintln!("gmx-atem: one tool call per line, such as: list {{}}");
    repl(&service.desk, std::io::stdin().lock(), std::io::stdout());
}

fn fail(message: &str) -> ! {
    eprintln!("gmx-atem: {message}");
    std::process::exit(2);
}
