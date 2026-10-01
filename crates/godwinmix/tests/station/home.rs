//! `GODWINMIX_HOME` moves every per user path, for the station and for `gmx`
//! alike, and `gmx plugin add` polls its task the way a plain REST client
//! does. The test points `HOME` at an empty folder and checks that nothing
//! lands in it: a scratch home that leaked into the real one is how a plugin
//! once got installed into an operator's own `~/.godwinmix/plugins`.

use super::support::*;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const GMX: &str = env!("CARGO_BIN_EXE_gmx");

/// The per user variables a station and `gmx` both get: a fake `HOME` that
/// must stay empty, and a `GODWINMIX_HOME` beside it.
fn homes(cmd: &mut Command, dir: &Path) {
    cmd.env("HOME", dir.join("fake-home"))
        .env("USERPROFILE", dir.join("fake-home"))
        .env("GODWINMIX_HOME", dir.join("gm-home"))
        .env_remove("GODWINMIX_PLUGINS_DIR")
        .env_remove("GODWINMIX_BUS_DIR")
        .env_remove("XDG_RUNTIME_DIR")
        .env_remove("GODWINMIX_TOKEN")
        // GStreamer keeps its registry under HOME unless told otherwise.
        .env("GST_REGISTRY", dir.join("gst-registry.bin"))
        .env("XDG_CACHE_HOME", dir.join("cache"))
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .env("XDG_DATA_HOME", dir.join("data"))
        .env("GODWINMIX_RUNTIME_DIR", dir.join("runtime"));
}

/// The first run config with nothing naming a plugins folder, so the only
/// thing that can place one is `GODWINMIX_HOME`.
fn folder_without_plugins_dir(name: &str) -> (PathBuf, u16) {
    let dir = std::env::temp_dir().join(format!("gmx-home-test-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("fake-home")).unwrap();
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let example = Command::new(BIN).arg("--example-config").output().unwrap().stdout;
    let text = String::from_utf8(example).unwrap().replace("bind = \"0.0.0.0:8080\"", &format!("bind = \"127.0.0.1:{port}\""));
    assert!(!text.lines().any(|l| l.trim_start().starts_with("plugins_dir")), "the example must not set plugins_dir");
    std::fs::write(dir.join("godwinmix.toml"), text).unwrap();
    (dir, port)
}

struct Station(std::process::Child);

impl Drop for Station {
    fn drop(&mut self) {
        #[cfg(unix)]
        unsafe {
            libc::kill(self.0.id() as i32, libc::SIGINT);
        }
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(20) {
            if let Ok(Some(_)) = self.0.try_wait() {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn up(port: u16, dir: &Path) {
    let started = Instant::now();
    let client = reqwest::Client::new();
    loop {
        let url = format!("http://127.0.0.1:{port}/api/v1/core/status");
        if let Ok(r) = client.get(url).timeout(Duration::from_secs(5)).send().await {
            if r.status().is_success() {
                return;
            }
        }
        assert!(started.elapsed() < Duration::from_secs(90), "not up after 90 s; see {}", dir.join("log.jsonl").display());
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// Every file under `dir`, for a message that says what leaked.
fn everything_under(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            if e.path().is_dir() {
                stack.push(e.path());
            }
            out.push(e.path());
        }
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gmx_plugin_add_installs_under_godwinmix_home_and_never_touches_home() {
    let (dir, port) = folder_without_plugins_dir("plugin-add");
    let mut cmd = Command::new(BIN);
    cmd.arg("--config").arg(dir.join("godwinmix.toml")).args(["--log-format", "json"]);
    homes(&mut cmd, &dir);
    cmd.stdout(Stdio::null()).stderr(Stdio::from(std::fs::File::create(dir.join("log.jsonl")).unwrap()));
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let _station = Station(cmd.spawn().unwrap());
    up(port, &dir).await;

    let plugin = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/zero-dep").canonicalize().unwrap();
    let mut gmx = Command::new(GMX);
    gmx.args(["plugin", "--url", &format!("http://127.0.0.1:{port}"), "add"]).arg(&plugin);
    homes(&mut gmx, &dir);
    let out = tokio::task::spawn_blocking(move || gmx.output().unwrap()).await.unwrap();
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "gmx plugin add failed: {said}");
    assert!(!said.contains("duplicate field"), "{said}");

    assert!(dir.join("gm-home/plugins/zero-dep").is_dir(), "installed under GODWINMIX_HOME: {said}");
    let listed = get_at(port, "/api/v1/plugins").await;
    assert!(listed["plugins_dir"].as_str().unwrap_or_default().starts_with(dir.join("gm-home").to_str().unwrap()), "{listed}");

    // A plain REST client reads the same task by its path, with or without
    // the id repeated as a parameter, and two ids that disagree are named.
    let tasks = get_at(port, "/api/v1/tasks").await;
    let id = tasks.as_array().and_then(|t| t.iter().find(|t| t["kind"] == "plugin.add")).map(|t| t["task_id"].as_str().unwrap().to_string());
    let id = id.unwrap_or_else(|| panic!("no plugin.add task in {tasks}"));
    assert_eq!(get_at(port, &format!("/api/v1/tasks/{id}")).await["state"], "completed");
    assert_eq!(get_at(port, &format!("/api/v1/tasks/{id}?task_id={id}")).await["state"], "completed");
    let both = get_at(port, &format!("/api/v1/tasks/{id}?task_id=other")).await;
    assert!(both.to_string().contains("two different tasks"), "{both}");

    let leaked = everything_under(&dir.join("fake-home"));
    assert!(leaked.is_empty(), "HOME must stay empty, found {leaked:?}");
}

async fn get_at(port: u16, path: &str) -> serde_json::Value {
    let url = format!("http://127.0.0.1:{port}{path}");
    let r = reqwest::Client::new().get(url).timeout(Duration::from_secs(20)).send().await.unwrap();
    r.json().await.unwrap_or_default()
}
