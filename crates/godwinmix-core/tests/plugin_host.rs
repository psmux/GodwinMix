//! The plugin host against a real Python process, on every platform.
//!
//! What a plugin author hit building a Python service on Windows: replies with
//! a curly quote in them ended the channel, `plugin.remove` was refused with
//! "Access is denied" while the plugin's interpreter was still running from its
//! folder, a reinstall of the same version failed the same way, and `GMX_RPC`
//! was empty. The fixture is `tests/fixtures/py-service`, standard library
//! only, installed into a venv as `gmx plugin add` would.

use godwinmix_core::caps::CanvasCaps;
use godwinmix_core::plugin::loader;
use godwinmix_core::plugin::supervisor::{retire, Supervisor};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/py-service")
}

fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Python is needed, as it is for the plugin. A machine without one says so.
fn python_here() -> bool {
    let python = godwinmix_host::python::interpreter();
    let found = std::process::Command::new(&python[0]).args(&python[1..]).arg("--version").output();
    let ok = found.is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("no Python on this machine; the Python plugin host tests are skipped");
    }
    ok
}

fn install(tag: &str) -> (PathBuf, Arc<Supervisor>) {
    let dir = std::env::temp_dir().join(format!("gmx-pyhost-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a temporary directory");
    loader::set_dir(dir.clone());
    loader::set_runtime_dir(dir.join("run"));
    loader::install_from_path(&fixture()).expect("installing the fixture");
    let canvas = CanvasCaps::new(&godwinmix_core::config::Canvas { width: 320, height: 180, fps: godwinmix_core::config::FrameRate::whole(30), sample_rate: 48_000, channels: 2 });
    let supervisor = Supervisor::new(canvas, Default::default());
    let failures = supervisor.start_all();
    assert!(failures.is_empty(), "the service would not start: {failures:?}");
    (dir, supervisor)
}

fn pid_of_service() -> u32 {
    let processes = retire::processes("pyservice");
    assert_eq!(processes.len(), 1, "one singleton, one process: {processes:?}");
    processes[0].1
}

fn echo(supervisor: &Supervisor, tool: &str) -> Value {
    supervisor.tool_call(&format!("pyservice/{tool}"), json!({"say": "hi"})).expect("the tool call")
}

#[test]
fn a_python_plugin_speaks_utf8_and_a_line_that_is_not_leaves_the_channel_working() {
    let _lock = exclusive();
    if !python_here() {
        return;
    }
    let (dir, supervisor) = install("utf8");
    let answer = echo(&supervisor, "echo");
    assert_eq!(answer["content"][0]["text"], "the Lord\u{2019}s \u{201c}prayer\u{201d}, caf\u{e9}");
    let env = &answer["structuredContent"]["env"];
    if std::env::var_os("PYTHONUTF8").is_none() {
        assert_eq!(env["PYTHONUTF8"], "1", "{env}");
    }
    // A line in the Windows code page, then the reply: the reply arrives, and
    // so does the next one. Before, the reader stopped at the bad line on
    // Windows and every later reply waited out its deadline.
    let raw = echo(&supervisor, "raw");
    assert_eq!(raw["structuredContent"]["arguments"]["say"], "hi");
    assert_eq!(echo(&supervisor, "echo")["structuredContent"]["arguments"]["say"], "hi");
    supervisor.shutdown();
    let _ = retire::remove_folder(&dir);
}

#[test]
fn removing_a_running_python_plugin_waits_for_its_process_and_leaves_no_folder() {
    let _lock = exclusive();
    if !python_here() {
        return;
    }
    let (dir, supervisor) = install("remove");
    let pid = pid_of_service();
    let began = Instant::now();
    supervisor.retire("pyservice", "the plugin was removed", true).expect("retiring the plugin");
    let took = began.elapsed();
    assert!(retire::exited(pid, Instant::now()), "the interpreter {pid} is still running after retire");
    loader::uninstall("pyservice").expect("the folder goes once the process has");
    assert!(!dir.join("pyservice").exists(), "the plugin's folder is still there");
    eprintln!("retire took {took:?} for a plugin that lingers 1.5 s on shutdown");
    assert!(took < retire::RETIRE_WAIT, "{took:?}");
    let _ = retire::remove_folder(&dir);
}

#[test]
fn reinstalling_the_running_version_stops_the_old_process_first_and_the_new_one_starts() {
    let _lock = exclusive();
    if !python_here() {
        return;
    }
    let (dir, supervisor) = install("reinstall");

    let old = pid_of_service();
    let retiring = supervisor.clone();
    let opts = loader::InstallOptions {
        retire: Some(loader::Retirer(Arc::new(move |name: &str, folder: bool| retiring.retire(name, "reinstalled", folder)))),
        ..Default::default()
    };
    loader::install(&fixture().to_string_lossy(), &opts).expect("reinstalling the same version over a running copy");
    assert!(retire::exited(old, Instant::now() + Duration::from_secs(1)), "the old interpreter {old} outlived its reinstall");
    let failures = supervisor.start_all();
    assert!(failures.is_empty(), "the new copy would not start: {failures:?}");
    let new = pid_of_service();
    assert_ne!(old, new, "a new process runs the new copy");
    supervisor.shutdown();
    let _ = retire::exited(new, Instant::now() + retire::RETIRE_WAIT);
    let _ = retire::remove_folder(&dir);
}

#[test]
fn a_plugin_is_told_where_the_core_is_and_given_a_token() {
    let _lock = exclusive();
    if !python_here() {
        return;
    }
    loader::set_rpc_url("ws://127.0.0.1:23399/rpc".into());
    let tokens = godwinmix_protocol::scope::Tokens::new(Vec::new(), false);
    loader::set_token_minter(Box::new(move |plugin, instance| tokens.mint_for_plugin(plugin, instance, None)));
    let (dir, supervisor) = install("rpc");
    let env = echo(&supervisor, "echo")["structuredContent"]["env"].clone();
    assert_eq!(env["GMX_RPC"], "ws://127.0.0.1:23399/rpc", "{env}");
    assert_eq!(env["token"], true, "{env}");
    supervisor.retire("pyservice", "the test is over", true).expect("stopping the service");
    let _ = retire::remove_folder(&dir);
}
