//! `gmx-ptz`, the process.
//!
//! Started by the core as a sidecar, or by hand to drive cameras from a
//! terminal with one tool call per line. See the head of
//! `plugins/osc/src/main.rs` for why both exist.

use godwinmix_sdk::prelude::*;
use gmx_ptz::core::Core;
use gmx_ptz::rig::Rig;
use gmx_ptz::service::{repl, PtzService};
use gmx_ptz::Settings;
use serde_json::{json, Value};

const USAGE: &str = "\
gmx-ptz: PTZ cameras for GodwinMix.

Started by the core as a sidecar, or by hand, reading one tool call per line:

  gmx-ptz --url http://127.0.0.1:8080 --token TOKEN \\
          --camera cam-pulpit:visca-ip:192.0.2.20:pulpit
  move {\"camera\": \"cam-pulpit\", \"pan\": 0.3}
  preset_save {\"camera\": \"cam-pulpit\", \"preset\": 1, \"name\": \"Pulpit\"}
  track_follow {\"camera\": \"cam-pulpit\", \"framing\": \"waist\"}

  --url <url>         the mixer's address, for pictures (env GODWINMIX_URL)
  --token <token>     bearer token (env GODWINMIX_TOKEN)
  --camera <spec>     id:protocol:address[:source]; repeatable. The protocol is
                      visca-ip, visca-tcp, onvif, canon or fake
  --user <name>       the login for the cameras given after it
  --password <pw>
  --settings <file>   a JSON file shaped like schemas/ptz.json
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
        println!("gmx-ptz {}", env!("CARGO_PKG_VERSION"));
        return;
    }
    let env = PluginEnv::from_env();
    if env.started_by_core() {
        run_as_sidecar(env);
    } else {
        run_standalone(argv);
    }
}

fn run_as_sidecar(env: PluginEnv) {
    let manifest = Manifest::load(env.root.join("gmx-plugin.toml")).expect("gmx-plugin.toml is beside the binary the core started");
    if let Err(error) = runtime::run(&manifest, ServiceHandler(PtzService { env, rig: None })) {
        eprintln!("gmx-ptz stopped: {error}");
        std::process::exit(1);
    }
}

fn run_standalone(argv: Vec<String>) {
    let mut url = std::env::var("GODWINMIX_URL").ok();
    let mut token = std::env::var("GODWINMIX_TOKEN").ok();
    let mut settings = json!({});
    let mut cameras: Vec<Value> = Vec::new();
    let (mut user, mut password) = (String::new(), String::new());
    let mut it = argv.into_iter();
    while let Some(flag) = it.next() {
        let mut value = || it.next().unwrap_or_default();
        match flag.as_str() {
            "--url" => url = Some(value()),
            "--token" => token = Some(value()),
            "--user" => user = value(),
            "--password" => password = value(),
            "--camera" => cameras.push(camera_spec(&value(), &user, &password)),
            "--settings" => {
                let path = value();
                let text = std::fs::read_to_string(&path).unwrap_or_else(|e| fail(&format!("cannot read {path}: {e}")));
                settings = serde_json::from_str(&text).unwrap_or_else(|e| fail(&format!("{path} is not JSON: {e}")));
            }
            other => fail(&format!("'{other}' is not a flag this program takes.\n\n{USAGE}")),
        }
    }
    if !cameras.is_empty() {
        settings["cameras"] = Value::Array(cameras);
    }
    let core = url.as_deref().and_then(|u| Core::from_url(u, token));
    let mut rig = Rig::new(Settings::from_value(&settings), core);
    for p in &rig.problems {
        eprintln!("gmx-ptz: {p}");
    }
    eprintln!("gmx-ptz: {} camera(s): {}. One tool call per line, such as: camera_list {{}}", rig.cameras.len(), rig.ids().join(", "));
    repl(&mut rig, std::io::stdin().lock(), std::io::stdout());
}

/// `id:protocol:address[:source]`. An address with a port keeps it:
/// `cam-1:visca-ip:192.0.2.20:52381:pulpit` has address `192.0.2.20:52381`.
fn camera_spec(text: &str, user: &str, password: &str) -> Value {
    let parts: Vec<&str> = text.split(':').collect();
    let (id, protocol) = (parts.first().copied().unwrap_or(""), parts.get(1).copied().unwrap_or(""));
    let rest = &parts[parts.len().min(2)..];
    let (address, source) = match rest {
        [host, port, source] if port.parse::<u16>().is_ok() => (format!("{host}:{port}"), Some(*source)),
        [host, port] if port.parse::<u16>().is_ok() => (format!("{host}:{port}"), None),
        [host, source] => (host.to_string(), Some(*source)),
        [host] => (host.to_string(), None),
        _ => (rest.join(":"), None),
    };
    json!({"id": id, "protocol": protocol, "address": address, "source": source, "user": user, "password": password})
}

fn fail(message: &str) -> ! {
    eprintln!("gmx-ptz: {message}");
    std::process::exit(2);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_camera_spec_reads_with_and_without_a_port_and_a_source() {
        assert_eq!(camera_spec("cam-1:visca-ip:192.0.2.20", "", "")["address"], "192.0.2.20");
        let v = camera_spec("cam-1:visca-ip:192.0.2.20:52381:pulpit", "u", "p");
        assert_eq!((v["address"].as_str(), v["source"].as_str(), v["user"].as_str()), (Some("192.0.2.20:52381"), Some("pulpit"), Some("u")));
        assert_eq!(camera_spec("cam-1:canon:192.0.2.21:wide", "", "")["source"], "wide");
    }
}
