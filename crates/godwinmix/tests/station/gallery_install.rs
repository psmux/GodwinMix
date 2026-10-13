//! A pure Python plugin installed the way the gallery installs one: from a
//! GitHub release whose one asset is for any platform and holds source, with
//! no restart. The test serves the release itself on loopback, then checks
//! that the venv was made, the plugin's service is running, its panel is in
//! `/plugins/index.json` and its page is served, and the install task said
//! which stage it reached.

use super::release_server::serve_release;
use super::support::*;
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, Instant};

const NAME: &str = "psalm-panel";

const MANIFEST: &str = r#"[plugin]
name = "psalm-panel"
version = "1.0.0"
api = 1
description = "A verse on a panel, in Python with nothing but the standard library."
license = "MIT"
platforms = ["any"]
placements = ["sidecar"]
process = "singleton"

[run]
python = "main.py"

[[provides]]
kind = "service"
id = "verses"
settings = "settings.json"

[[provides]]
kind = "panel"
id = "verses-panel"
panel = { kind = "iframe", entry = "ui/verses.html", slots = ["sidebar"] }
"#;

/// The smallest service: say hello, answer everything, leave on shutdown.
/// Control is on stdin and stderr; stdout is never written.
const MAIN: &str = r#"import json, sys
def send(o):
    sys.stderr.write(json.dumps(o) + "\n"); sys.stderr.flush()
send({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"plugin": "psalm-panel",
      "version": "1.0.0", "api": 1, "transports": [], "provides": [{"kind": "service", "id": "verses"}]}})
for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if m.get("id") == 0 and "result" in m:
        send({"jsonrpc": "2.0", "method": "initialized", "params": {}})
    elif "method" in m and m.get("id") is not None:
        send({"jsonrpc": "2.0", "id": m["id"], "result": {"state": "ok"} if m["method"] == "health" else {}})
        if m["method"] == "shutdown":
            break
"#;

/// The plugin as its author's CI would pack it: `psalm-panel-1.0.0-any.tar.gz`.
fn release_asset(dir: &Path) -> (String, Vec<u8>) {
    let src = dir.join("build").join(NAME);
    std::fs::create_dir_all(src.join("ui")).unwrap();
    std::fs::write(src.join("gmx-plugin.toml"), MANIFEST).unwrap();
    std::fs::write(src.join("main.py"), MAIN).unwrap();
    std::fs::write(src.join("settings.json"), r#"{"type": "object", "properties": {}}"#).unwrap();
    std::fs::write(src.join("ui/verses.html"), "<!doctype html><p>In the beginning</p>").unwrap();
    let asset = format!("{NAME}-1.0.0-any.tar.gz");
    let out = dir.join(&asset);
    let made = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&out)
        .arg("-C")
        .arg(dir.join("build"))
        .arg(NAME)
        .status()
        .expect("tar is on every platform this runs on");
    assert!(made.success(), "tar could not pack the plugin");
    (asset, std::fs::read(out).unwrap())
}

async fn finished(ws: &mut Ws, task: &str) -> Value {
    let started = Instant::now();
    let mut id = 100;
    loop {
        id += 1;
        let view = call(ws, id, "task.get", json!({ "task_id": task })).await["result"].clone();
        if view["state"] != "running" {
            return view;
        }
        assert!(started.elapsed() < Duration::from_secs(240), "the install is still running: {view}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_python_plugin_from_an_any_platform_release_runs_and_shows_its_panel_with_no_restart() {
    let (dir, port) = folder("gallery-install");
    let (asset, bytes) = release_asset(&dir);
    let api = serve_release(asset, bytes);
    let st = start_with_env(dir.clone(), port, &[], &[("GMX_GITHUB_API", &api), ("GMX_NO_COSIGN", "1")]).await;
    let mut ws = rpc(&st, "").await;

    let handle = call(&mut ws, 1, "plugin.add", json!({ "source": format!("psmux/{NAME}") })).await;
    let task = handle["result"]["task_id"].as_str().unwrap_or_else(|| panic!("no task: {handle}")).to_string();
    let done = finished(&mut ws, &task).await;
    assert_eq!(done["state"], "completed", "the install failed: {done}; see {}", dir.join("log.jsonl").display());
    assert_eq!(done["stage"], "starting", "the last stage it reported: {done}");

    let root = dir.join("plugins").join(NAME).join("1.0.0");
    let venv = if cfg!(windows) { root.join(".venv/Scripts/python.exe") } else { root.join(".venv/bin/python") };
    assert!(venv.is_file(), "the venv was made at {}", venv.display());

    // Running with no restart: the service has a pid and is ready.
    let started = Instant::now();
    loop {
        let list = call(&mut ws, 2, "plugin.list", json!({})).await;
        let mine = list["result"]["plugins"].as_array().and_then(|p| p.iter().find(|p| p["name"] == NAME).cloned());
        let ready = mine.as_ref().and_then(|p| p["instances"].as_array().cloned()).unwrap_or_default();
        if ready.iter().any(|i| i["state"] == "ready" && i["pid"].is_u64()) {
            assert_eq!(mine.unwrap()["provide_kinds"][format!("{NAME}/verses")], "service");
            break;
        }
        assert!(started.elapsed() < Duration::from_secs(30), "the service never started: {mine:?}");
        tokio::time::sleep(Duration::from_millis(250)).await;
    }

    let index = get(&st, "/plugins/index.json").await;
    let panel = index["plugins"].as_array().and_then(|p| p.iter().find(|p| p["name"] == NAME).cloned());
    let panel = panel.unwrap_or_else(|| panic!("no panel for {NAME} in {index}"));
    assert_eq!(panel["id"], format!("{NAME}/verses-panel"));
    assert_eq!(panel["tier"], "sandboxed");
    let page = panel["page"].as_str().unwrap().to_string();
    let html = reqwest::get(format!("http://{}{page}", st.url)).await.unwrap().text().await.unwrap();
    assert!(html.contains("In the beginning"), "{page} served: {html}");
}
