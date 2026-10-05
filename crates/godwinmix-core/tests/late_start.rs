//! A source whose plugin answers `start` too late at boot comes up by itself.
//!
//! On 2026-10-05 a USB webcam answered `start` after more than five seconds at
//! every boot of the desktop app. The core gave up on it, logged "failed to
//! add source", stopped the plugin with "the source was removed", and never
//! asked again. The camera was in the saved config and missing from every
//! show.
//!
//! The plugin here is `examples/zero-dep`, the Python plugin with no imports
//! outside the standard library, changed so that the first `start` of the
//! test sleeps past the core's five seconds. The second process answers at
//! once. The source must fail at boot, stay wanted, and go live on a retry
//! with nobody asking. Python runs on every platform the core does, which a
//! shell plugin does not. Skips where there is no Python 3.10 or later.

use godwinmix_core::config::SourceConfig;
use godwinmix_core::plugin::loader;
use godwinmix_core::prelude::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const MANIFEST: &str = r#"
[plugin]
name = "latebars"
version = "0.1.0"
api = 1
description = "Colour bars whose first start answers late, for the late start test."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64", "windows-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
python = "main.py"

[[provides]]
kind = "source"
id = "source"
media = { video = "raw", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
"#;

/// Put into the plugin ahead of everything else: the first `start` it is
/// asked for in this test sleeps past the core's budget, once.
const LATE: &str = r#"
import os
def late_once():
    marker = os.environ.get("GMX_LATE_MARKER")
    if marker and not os.path.exists(marker):
        open(marker, "w").close()
        time.sleep(7)
"#;

/// A Python of 3.10 or later, as `GMX_PYTHON` names it for the loader.
fn python() -> Option<String> {
    let mut tries: Vec<String> = std::env::var("GMX_PYTHON").ok().into_iter().collect();
    tries.extend(["python3".to_string(), "python".to_string()]);
    tries.into_iter().find(|p| {
        std::process::Command::new(p)
            .args([
                "-c",
                "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)",
            ])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn install(root: &Path) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/zero-dep");
    let main = std::fs::read_to_string(example.join("main.py")).expect("the example plugin");
    let main = main
        .replace("\"zero-dep\"", "\"latebars\"")
        .replacen("API = 1\n", &format!("{LATE}\nAPI = 1\n"), 1)
        .replacen(
            "def on_start(rid, params):\n",
            "def on_start(rid, params):\n    late_once()\n",
            1,
        );
    assert!(
        main.contains("    late_once()"),
        "the example plugin changed shape"
    );
    let checkout = root.join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("gmx-plugin.toml"), MANIFEST).unwrap();
    std::fs::copy(
        example.join("settings.json"),
        checkout.join("settings.json"),
    )
    .unwrap();
    std::fs::write(checkout.join("main.py"), main).unwrap();
    loader::set_dir(root.join("plugins"));
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&checkout).expect("the plugin installs");
}

fn scratch() -> PathBuf {
    let root = std::env::temp_dir().join(format!("gmx-late-start-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_plugin_that_answers_start_late_at_boot_is_tried_again_and_goes_live() {
    let Some(python) = python() else {
        eprintln!("skipping: no Python 3.10 or later on PATH");
        return;
    };
    let _ = gstreamer::init();
    // The mixer's own account of the fail and the retry, when it is run with
    // --nocapture.
    let _ = tracing_subscriber::fmt().with_test_writer().try_init();
    let root = scratch();
    let marker = root.join("answered-late");
    std::env::set_var("GMX_PYTHON", &python);
    std::env::set_var("GMX_LATE_MARKER", &marker);
    install(&root);

    let mut cfg: Config = toml::from_str("").unwrap();
    (cfg.canvas.width, cfg.canvas.height, cfg.canvas.fps) = (320, 180, 30);
    let source: SourceConfig = toml::from_str("id = \"late\"\ntype = \"latebars/source\"").unwrap();
    cfg.sources.push(source);
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("a mixer");
    let booted = Instant::now();
    mix.start()
        .expect("the programme starts even though the source did not");
    let failed_at_boot = !mix.status().sources.iter().any(|s| s.id.as_str() == "late");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    let live = |s: &MixerStatus| {
        s.sources
            .iter()
            .any(|x| x.id.as_str() == "late" && x.state == SourceState::Live)
    };
    let mut came_up = None;
    while booted.elapsed() < Duration::from_secs(60) {
        if handle.status().await.is_ok_and(|s| live(&s)) {
            came_up = Some(booted.elapsed());
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let still_unstarted = handle
        .configs()
        .await
        .map(|c| c.unstarted.iter().any(|u| u.config.id == "late"))
        .unwrap_or(true);
    handle.send(Command::Shutdown).ok();
    tokio::task::spawn_blocking(move || thread.join())
        .await
        .ok();
    loader::uninstall("latebars").ok();
    std::env::remove_var("GMX_LATE_MARKER");

    assert!(
        marker.exists(),
        "the first start was never asked for, so this proved nothing"
    );
    assert!(
        failed_at_boot,
        "the first start answered in time, so this proved nothing"
    );
    let came_up = came_up.expect("the source never came up after its late start at boot");
    println!("failed at boot, live {came_up:?} after the mixer started");
    assert!(
        !still_unstarted,
        "a live source is still listed as unstarted"
    );
}
