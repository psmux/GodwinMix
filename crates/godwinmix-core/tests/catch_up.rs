//! A live source whose frames all arrive due well in the future is caught up.
//!
//! The camera sidecar on Windows carries its frames over a pipe in Matroska,
//! stamped from when its own pipeline started. Its first frames waited in that
//! pipe while the core linked the decoder, the core placed the source on the
//! programme's timeline when the first of them arrived, and every frame after
//! was then due as long after it arrived as that first one had waited: a
//! camera 0.6 to 1.6 s behind for good on 2026-10-06.
//!
//! The plugin here is `examples/zero-dep`, the Python plugin with no imports
//! outside the standard library, changed so that its first second and a half
//! of frames go out at once and the rest at the canvas rate: from the core,
//! the same thing as frames that waited in a pipe. The guard must notice, say
//! so in the log, and move the source back by about that much. Skips where
//! there is no Python 3.10 or later.

use godwinmix_core::config::SourceConfig;
use godwinmix_core::plugin::loader;
use godwinmix_core::prelude::*;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const MANIFEST: &str = r#"
[plugin]
name = "aheadbars"
version = "0.1.0"
api = 1
description = "Colour bars stamped a second and a half ahead, for the catch up test."
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

const AHEAD_MS: i64 = 1500;

fn python() -> Option<String> {
    let mut tries: Vec<String> = std::env::var("GMX_PYTHON").ok().into_iter().collect();
    tries.extend(["python3".to_string(), "python".to_string()]);
    tries.into_iter().find(|p| {
        std::process::Command::new(p)
            .args(["-c", "import sys; sys.exit(0 if sys.version_info >= (3, 10) else 1)"])
            .output()
            .is_ok_and(|o| o.status.success())
    })
}

fn install(root: &Path) {
    let example = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/zero-dep");
    let main = std::fs::read_to_string(example.join("main.py")).expect("the example plugin");
    let paced = "        pts = index * step_ns\n        wait = started + pts / 1e9 - time.monotonic()\n";
    assert!(main.contains(paced), "the example plugin changed shape");
    // The first second and a half of frames go out at once, as frames that
    // waited in a pipe do, and the rest at the canvas rate after them.
    let ahead = format!(
        "        pts = index * step_ns\n        wait = started - {} + pts / 1e9 - time.monotonic()\n",
        AHEAD_MS as f64 / 1000.0
    );
    let main = main.replace("\"zero-dep\"", "\"aheadbars\"").replacen(paced, &ahead, 1);
    let checkout = root.join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("gmx-plugin.toml"), MANIFEST).unwrap();
    std::fs::copy(example.join("settings.json"), checkout.join("settings.json")).unwrap();
    std::fs::write(checkout.join("main.py"), main).unwrap();
    loader::set_dir(root.join("plugins"));
    loader::set_runtime_dir(root.join("run"));
    loader::install_from_path(&checkout).expect("the plugin installs");
}

/// Everything the mixer logs, kept to be read back.
#[derive(Clone, Default)]
struct Log(Arc<Mutex<Vec<u8>>>);

impl std::io::Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
}

/// The number after `key` on the first, or the last, line holding `marker`.
fn number(text: &str, marker: &str, key: &str, last: bool) -> Option<i64> {
    let mut lines = text.lines().filter(|l| l.contains(marker));
    let line = if last { lines.last()? } else { lines.next()? };
    let at = line.find(key)? + key.len();
    line[at..].split(|c: char| !c.is_ascii_digit() && c != '-').next()?.parse().ok()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_whose_frames_all_wait_is_moved_back_to_its_newest() {
    let Some(python) = python() else {
        eprintln!("skipping: no Python 3.10 or later on PATH");
        return;
    };
    let _ = gstreamer::init();
    let log = Log::default();
    let writer = log.clone();
    let _ = tracing_subscriber::fmt().with_max_level(tracing::Level::DEBUG).with_ansi(false).with_writer(move || writer.clone()).try_init();
    let root = std::env::temp_dir().join(format!("gmx-catch-up-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::env::set_var("GMX_PYTHON", &python);
    install(&root);

    let mut cfg: Config = toml::from_str("").unwrap();
    (cfg.canvas.width, cfg.canvas.height, cfg.canvas.fps) = (320, 180, 30);
    cfg.sources.push(toml::from_str::<SourceConfig>("id = \"ahead\"\ntype = \"aheadbars/source\"").unwrap());
    let (mut mix, handle, cmd_rx, _bus) = Mixer::build(cfg).expect("a mixer");
    mix.start().expect("the programme starts");
    let thread = mixer::spawn(mix, cmd_rx, handle.clone());

    let started = Instant::now();
    let mut caught = None;
    while started.elapsed() < Duration::from_secs(30) && caught.is_none() {
        tokio::time::sleep(Duration::from_millis(250)).await;
        caught = number(&String::from_utf8_lossy(&log.0.lock().unwrap()), "dropped to the newest", "caught_up_ms=", false);
    }
    let took = started.elapsed();
    tokio::time::sleep(Duration::from_secs(3)).await;
    let live = handle.status().await.is_ok_and(|s| s.sources.iter().any(|x| x.id.as_str() == "ahead" && x.state == SourceState::Live));
    handle.send(Command::Shutdown).ok();
    tokio::task::spawn_blocking(move || thread.join()).await.ok();
    loader::uninstall("aheadbars").ok();
    let text = String::from_utf8_lossy(&log.0.lock().unwrap()).to_string();
    let _ = std::fs::remove_dir_all(&root);

    let caught = caught.unwrap_or_else(|| panic!("no catch up within 30 s; the log said:\n{text}"));
    println!("caught up by {caught} ms, {took:?} after the mixer started");
    assert!((AHEAD_MS - 300..=AHEAD_MS + 300).contains(&caught), "frames {AHEAD_MS} ms behind were caught up by {caught} ms");
    let after = number(&text, "least lead this tick", "lead_ms=Some(", true).expect("the guard's per tick line");
    assert!(after < 200, "the source was still {after} ms ahead after its catch up");
    assert!(!text.contains("it is left as it is"), "a live source was judged not live:\n{text}");
    assert!(live, "the source was not live after its catch up");
}
