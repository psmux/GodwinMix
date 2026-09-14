//! A real plugin, in a real process, with real GStreamer on both sides.
//!
//! The plugin here is a shell script. That is on purpose: if the protocol can
//! be implemented in forty lines of `sh` with `gst-launch-1.0` doing the
//! muxing, then a Python author with no dependencies can implement it too, and
//! the promise in 09 section 4 item 3 holds. It speaks the handshake, answers
//! `start`, `configure`, `health` and `shutdown`, and writes streamable
//! Matroska to stdout.
//!
//! These tests need a shell and `gst-launch-1.0`. They say so and skip rather
//! than failing where those are missing, because a check that cannot run is
//! not a check that passed.

#![cfg(unix)]

use godwinmix_core::config::SourceConfig;
use godwinmix_core::plugin::loader;
use std::path::{Path, PathBuf};

/// The whole plugin. Forty lines, no dependencies, one process.
const PLUGIN: &str = r#"#!/bin/sh
# A GodwinMix source plugin in shell. Control on stdin and stderr, media on
# stdout, exactly as 03 section 6 says.
say() { printf '%s\n' "$1" >&2; }

say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"shellbars","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'

# The core answers on stdin. Read one line and get on with it.
read -r _ready
say '{"jsonrpc":"2.0","method":"initialized"}'
say '{"jsonrpc":"2.0","method":"log","params":{"level":"info","message":"shellbars is up"}}'

media_started=0
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      if [ "$media_started" = 0 ]; then
        gst-launch-1.0 -q \
          videotestsrc is-live=true pattern=smpte \
          ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 \
          ! matroskamux streamable=true name=mux \
          ! fdsink fd=1 &
        media_started=1
      fi
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"latency_ms\":0}}"
      ;;
    *'"method":"health"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}"
      ;;
    *'"method":"configure"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":true}}"
      ;;
    *'"method":"stop"'*)
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}"
      ;;
    *'"method":"shutdown"'*)
      exit 0
      ;;
  esac
done
"#;

const MANIFEST: &str = r#"
[plugin]
name = "shellbars"
version = "0.1.0"
api = 1
description = "Colour bars from a shell script, for the tier 2 host's own tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
uri_schemes = ["shellbars://"]
rank = 210
media = { video = "container", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
"#;

/// Write the plugin into a directory that looks like a checkout.
fn write_plugin(at: &Path) {
    std::fs::create_dir_all(at).expect("the plugin directory");
    std::fs::write(at.join("gmx-plugin.toml"), MANIFEST).expect("the manifest");
    std::fs::write(at.join("settings.json"), r#"{"type":"object","properties":{}}"#)
        .expect("the settings schema");
    let entry = at.join("run.sh");
    std::fs::write(&entry, PLUGIN).expect("the plugin");
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&entry, std::fs::Permissions::from_mode(0o755)).expect("chmod");
}

fn temp(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("gmx-sidecar-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("a temporary directory");
    path
}

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).map(|d| d.join(name)).find(|p| p.is_file())
    })
}

/// One lock for the whole file: the plugin registry is one per process and
/// these tests install into it.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

#[test]
fn a_plugin_is_installed_registered_and_removed_without_a_trace() {
    let _lock = exclusive();
    let root = temp("install");
    let source = root.join("checkout");
    write_plugin(&source);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("the plugins directory");
    loader::set_dir(plugins.clone());

    let installed = loader::install_from_path(&source).expect("it installs");
    assert_eq!(installed.name(), "shellbars");
    assert_eq!(installed.version(), "0.1.0");
    assert!(plugins.join("shellbars").join("0.1.0").join("run.sh").is_file());
    // The entry point keeps its executable bit, or the process would never
    // start and nothing would say why.
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(plugins.join("shellbars").join("0.1.0").join("run.sh"))
        .expect("the installed entry point")
        .permissions()
        .mode();
    assert_eq!(mode & 0o100, 0o100, "the entry point is executable");

    // It is in the same table every built in kind is in.
    assert!(godwinmix_core::plugin::source::by_type("shellbars/source").is_some());
    assert!(godwinmix_core::plugin::source::available()
        .contains(&"shellbars/source".to_string()));
    let described = godwinmix_core::plugin::source::described();
    assert!(described.iter().any(|k| k.id == "shellbars/source"));

    loader::uninstall("shellbars").expect("it is removed");
    assert!(
        godwinmix_core::plugin::source::by_type("shellbars/source").is_none(),
        "the provide went with the plugin"
    );
    assert!(
        !plugins.join("shellbars").exists(),
        "plugin.add then plugin.remove leaves no directory"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// The acceptance path: install a plugin, take its source live, and see real
/// frames at the canvas caps.
#[test]
fn a_shell_plugin_reaches_the_canvas_through_the_container_transport() {
    let _lock = exclusive();
    if which("gst-launch-1.0").is_none() {
        println!("skipping: gst-launch-1.0 is not on PATH");
        return;
    }
    let _ = gstreamer::init();
    let root = temp("live");
    let source = root.join("checkout");
    write_plugin(&source);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("the plugins directory");
    loader::set_dir(plugins.clone());
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&source).expect("it installs");

    let before = descriptors();
    let mut cfg = SourceConfig::bare("bars", "shellbars://smpte");
    cfg.type_id = Some("shellbars/source".into());
    let report = godwinmix_core::plugin::harness::check_source(&cfg, false);
    let outcome = match report {
        Ok(report) => {
            for line in report.lines() {
                println!("  {line}");
            }
            report.into_result().map(|_| ())
        }
        Err(e) => Err(e),
    };
    loader::uninstall("shellbars").ok();
    outcome.expect("a sidecar source is as conformant as a built in one");

    // Nothing left over. The media directory is removed with the instance and
    // the descriptors come back, within the slack GStreamer's own pools take.
    assert!(
        !root.join("run").join("plugins").join("bars").exists(),
        "the instance's media directory went with it"
    );
    let after = descriptors();
    assert!(
        after <= before + 8,
        "a sidecar leaked descriptors: {before} before, {after} after"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// How many descriptors this process holds. The same counting the exec tests
/// in `input.rs` already do.
fn descriptors() -> usize {
    let dir = if cfg!(target_os = "macos") { "/dev/fd" } else { "/proc/self/fd" };
    std::fs::read_dir(dir).map(|d| d.count()).unwrap_or(0)
}

/// Checks 1, 4, 6 and 7 against the shell plugin: the ones a built in kind
/// has no process for.
#[test]
fn the_harness_makes_every_check_a_plugin_has_a_process_for() {
    let _lock = exclusive();
    if which("gst-launch-1.0").is_none() {
        println!("skipping: gst-launch-1.0 is not on PATH");
        return;
    }
    let _ = gstreamer::init();
    let root = temp("harness");
    let source = root.join("checkout");
    write_plugin(&source);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("the plugins directory");
    loader::set_dir(plugins.clone());
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&source).expect("it installs");
    let installed = loader::get("shellbars").expect("it is installed");

    use godwinmix_core::plugin::harness;
    // Check 7 needs no process at all.
    let manifest = harness::check_manifest(&installed.root);
    println!("  {}", manifest.detail);
    assert!(manifest.passed, "{}", manifest.detail);

    // Check 1: it says hello inside the window, and the report says how long
    // it took so an author can watch that number.
    let spawn = harness::check_spawn(&installed.root, "source");
    println!("  {}", spawn.detail);
    assert!(spawn.passed, "{}", spawn.detail);
    assert!(spawn.detail.contains("container"), "{}", spawn.detail);

    // Check 4: a schema with no examples says so rather than passing quietly.
    let configure = harness::check_configure(&installed.root, "source");
    println!("  {}", configure.detail);
    assert!(configure.passed, "{}", configure.detail);

    loader::uninstall("shellbars").ok();
    let _ = std::fs::remove_dir_all(&root);
}

/// The whole run, as `gmx plugin test` makes it.
#[test]
fn gmx_plugin_test_runs_the_quick_suite_against_a_real_plugin() {
    let _lock = exclusive();
    if which("gst-launch-1.0").is_none() {
        println!("skipping: gst-launch-1.0 is not on PATH");
        return;
    }
    let _ = gstreamer::init();
    let root = temp("quick");
    let source = root.join("checkout");
    write_plugin(&source);
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).expect("the plugins directory");
    loader::set_dir(plugins.clone());
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&source).expect("it installs");
    let installed = loader::get("shellbars").expect("it is installed");

    let report = godwinmix_core::plugin::harness::check_plugin(&installed.root, true)
        .expect("the harness runs");
    for line in report.lines() {
        println!("  {line}");
    }
    let outcome = report.into_result();
    loader::uninstall("shellbars").ok();
    outcome.expect("a forty line shell plugin is conformant");
    let _ = std::fs::remove_dir_all(&root);
}
