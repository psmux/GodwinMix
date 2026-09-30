//! A device opened once and read by every source that names it, in this
//! process and in another one, with real plugin processes and real GStreamer.
//!
//! The plugin is a shell script that declares `share` and writes a line to a
//! log each time it opens its "device", so the tests can count opens. A
//! second mixer is this test binary started again as a child, which is what a
//! show will be: another process on the same machine with the same registry.
//!
//! These need a shell and `gst-launch-1.0`, and skip where those are missing.

#![cfg(unix)]

use godwinmix_core::config::{BrowserConfig, SourceConfig};
use godwinmix_core::plugin::source::{Source, SourceRequest};
use godwinmix_core::plugin::{harness, loader, Hello, MediaEnds, Tier, API_LEVEL};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const PLUGIN: &str = r#"#!/bin/sh
say() { printf '%s\n' "$1" >&2; }
say '{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"plugin":"sharebars","version":"0.1.0","api":1,"transports":["container"],"provides":[]}}'
read -r _ready
say '{"jsonrpc":"2.0","method":"initialized"}'
started=0
while IFS= read -r line; do
  id=$(printf '%s' "$line" | sed -n 's/.*"id":\([0-9]*\).*/\1/p')
  case "$line" in
    *'"method":"start"'*)
      if [ "$started" = 0 ]; then
        echo "open $$" >> "$SHAREBARS_LOG"
        gst-launch-1.0 -q videotestsrc is-live=true pattern=ball \
          ! video/x-raw,format=I420,width=640,height=360,framerate=30/1 \
          ! matroskamux streamable=true ! fdsink fd=1 &
        started=1
      fi
      say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"latency_ms\":0}}" ;;
    *'"method":"health"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"state\":\"ok\"}}" ;;
    *'"method":"configure"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{\"applied\":true}}" ;;
    *'"method":"stop"'*) say "{\"jsonrpc\":\"2.0\",\"id\":$id,\"result\":{}}" ;;
    *'"method":"shutdown"'*) kill %1 2>/dev/null; exit 0 ;;
  esac
done
kill %1 2>/dev/null
"#;

const MANIFEST: &str = r#"
[plugin]
name = "sharebars"
version = "0.1.0"
api = 1
description = "A pretend camera from a shell script, for the shared source tests."
license = "Apache-2.0"
platforms = ["linux-x86_64", "linux-aarch64", "macos-aarch64", "macos-x86_64"]
placements = ["sidecar"]
process = "per-instance"

[run]
shell = "run.sh"

[[provides]]
kind = "source"
id = "source"
media = { video = "container", audio = "none", alpha = false, thumb = true }
transports = ["container"]
capabilities = ["restart-in-place", "health"]
latency_ms = 0
settings = "settings.json"
share = { bus = "camera", params = ["device"] }
"#;

fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Install the plugin into a directory of its own, point the bus at another,
/// and say where the open log is. Everything a child needs is in `root`.
fn setup(root: &Path) {
    let checkout = root.join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("gmx-plugin.toml"), MANIFEST).unwrap();
    std::fs::write(checkout.join("settings.json"), r#"{"type":"object","properties":{}}"#).unwrap();
    std::fs::write(checkout.join("run.sh"), PLUGIN).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(checkout.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    loader::set_dir(plugins);
    loader::set_runtime_dir(root.join(format!("run-{}", std::process::id())));
    loader::install_from_path(&checkout).expect("it installs");
}

fn env_for(root: &Path) {
    std::env::set_var("GODWINMIX_BUS_DIR", root.join("bus"));
    std::env::set_var("SHAREBARS_LOG", root.join("opens.log"));
}

fn opens(root: &Path) -> usize {
    std::fs::read_to_string(root.join("opens.log")).map(|t| t.lines().count()).unwrap_or(0)
}

/// Frames seen at one source's programme end, and the longest gap between two.
#[derive(Default)]
struct Count {
    frames: AtomicU64,
    longest_us: AtomicU64,
    last: Mutex<Option<Instant>>,
}

impl Count {
    fn gap_reset(&self) {
        self.longest_us.store(0, Relaxed);
    }
    fn longest(&self) -> Duration {
        Duration::from_micros(self.longest_us.load(Relaxed))
    }
}

struct Running {
    source: Box<dyn Source>,
    ends: MediaEnds,
    count: Arc<Count>,
}

fn open(id: &str, device: &str) -> Running {
    let mut cfg = SourceConfig::bare(id, "");
    cfg.type_id = Some("sharebars/source".into());
    cfg.params.insert("device".into(), toml::Value::String(device.into()));
    let canvas = harness::test_canvas();
    let backends = godwinmix_core::probe::Backends::probe(
        godwinmix_core::config::Accel::Auto,
        godwinmix_core::config::Accel::Auto,
    )
    .unwrap();
    let browser = BrowserConfig::default();
    let provide = godwinmix_core::plugin::source::resolve_config(&cfg).unwrap();
    let req = SourceRequest {
        cfg: &cfg,
        canvas: &canvas,
        backends: &backends,
        browser: &browser,
        allow_exec: false,
        thumb_fps: 8,
        origin: Instant::now(),
        overlay: None,
    };
    let mut source = (provide.make)(req).unwrap();
    source
        .initialize(Hello {
            instance: id.into(),
            canvas: canvas.clone(),
            api_level: API_LEVEL,
            params: cfg.effective_params(),
            tier: Tier::Core,
        })
        .unwrap();
    let ends = source.start(&canvas, false).unwrap();
    let count = Arc::new(Count::default());
    let c = count.clone();
    ends.video.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        let now = Instant::now();
        if let Some(then) = c.last.lock().unwrap().replace(now) {
            c.longest_us.fetch_max(now.duration_since(then).as_micros() as u64, Relaxed);
        }
        c.frames.fetch_add(1, Relaxed);
        gst::PadProbeReturn::Ok
    });
    ends.pipeline.set_state(gst::State::Playing).unwrap();
    Running { source, ends, count }
}

impl Running {
    fn frames(&self) -> u64 {
        self.count.frames.load(Relaxed)
    }
    fn share(&mut self) -> serde_json::Value {
        self.source.call("share", serde_json::json!({})).unwrap()
    }
    fn close(mut self) {
        let _ = self.ends.pipeline.set_state(gst::State::Null);
        self.source.stop().unwrap();
    }
}

/// Wait until `f` holds, for at most `limit`.
fn until(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    f()
}

/// One test at a time: the plugin registry and the environment are per process.
fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

fn skip() -> bool {
    if which("gst-launch-1.0").is_none() {
        println!("skipping: gst-launch-1.0 is not on PATH");
        return true;
    }
    false
}

fn temp(tag: &str) -> PathBuf {
    let path = PathBuf::from(format!("/tmp/gmx-share-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// The second mixer: open the device, say so, count frames until killed or
/// until its stdin closes.
#[test]
fn child_entry() {
    let Ok(root) = std::env::var("SHARE_CHILD_ROOT") else { return };
    let root = PathBuf::from(root);
    let _ = gst::init();
    setup(&root);
    let mut cam = open("cam-b", "facetime");
    let owner = until(Duration::from_secs(10), || cam.share()["owner"] == true);
    println!("RESULT ready=1 owner={}", u8::from(owner));
    let mut line = String::new();
    let _ = std::io::stdin().read_line(&mut line);
    println!("RESULT frames={}", cam.frames());
    cam.close();
    std::process::exit(0);
}

#[test]
fn two_sources_in_one_process_open_the_device_once_and_the_second_takes_over() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("one");
    env_for(&root);
    setup(&root);

    let mut a = open("cam-a", "facetime");
    assert!(until(Duration::from_secs(10), || a.frames() > 10), "the first source shows a picture");
    let mut b = open("cam-a2", "facetime");
    assert!(until(Duration::from_secs(5), || b.frames() > 30), "the second reads the same picture");
    assert_eq!(opens(&root), 1, "the device was opened once for two sources");
    assert_eq!(a.share()["owner"], true);
    assert_eq!(b.share()["owner"], false);
    let (owner, reader) = (a.share(), b.share());
    println!("owner publishes in {}, reader holds it {} later", owner["publish_ms"], reader["hop_ms"]);
    // The owner's normaliser holds no frame: publishing takes well under one.
    let publish = owner["publish_ms"]["p50"].as_f64().expect("the owner timed its frames");
    assert!(publish < 10.0, "the feed held frames: {}", owner["publish_ms"]);
    assert!(reader["hop_ms"]["p50"].as_f64().unwrap() < 10.0, "{}", reader["hop_ms"]);

    b.count.gap_reset();
    let before = b.frames();
    a.close();
    assert!(
        until(Duration::from_secs(10), || b.frames() > before + 30),
        "the second source kept a picture after the first went"
    );
    assert_eq!(b.share()["owner"], true, "the reader took the device over");
    assert_eq!(opens(&root), 2);
    println!("handover gap in one process: {:?}", b.count.longest());
    assert!(b.count.longest() < Duration::from_secs(3), "gap {:?}", b.count.longest());
    b.close();
    loader::uninstall("sharebars").ok();
}

#[test]
fn a_mixer_killed_with_sigkill_hands_its_camera_to_the_one_reading_it() {
    let _lock = exclusive();
    if skip() {
        return;
    }
    let _ = gst::init();
    let root = temp("two");
    env_for(&root);
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "child_entry", "--nocapture", "--test-threads=1"])
        .env("SHARE_CHILD_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut lines = BufReader::new(child.stdout.take().unwrap());
    let result = |lines: &mut BufReader<std::process::ChildStdout>| -> String {
        let mut line = String::new();
        loop {
            line.clear();
            assert!(lines.read_line(&mut line).unwrap() > 0, "the child said nothing");
            if let Some((_, rest)) = line.split_once("RESULT ") {
                return rest.trim().to_string();
            }
        }
    };
    assert_eq!(result(&mut lines), "ready=1 owner=1", "the first mixer owns the device");

    // This process is the second mixer. It must read, not open.
    setup(&root);
    let mut b = open("cam-b", "facetime");
    assert!(until(Duration::from_secs(10), || b.frames() > 30), "the second mixer has a picture");
    assert_eq!(opens(&root), 1, "one open for two mixers");
    assert_eq!(b.share()["owner"], false);
    let hop = b.share()["hop_ms"].clone();
    println!("from the owner's publish to this process holding the frame: {hop}");
    assert!(hop["p50"].as_f64().unwrap() < 10.0, "{hop}");

    b.count.gap_reset();
    let before = b.frames();
    // SAFETY: a signal to our own child.
    unsafe { libc::kill(child.id() as i32, libc::SIGKILL) };
    let _ = child.wait();
    assert!(
        until(Duration::from_secs(10), || b.frames() > before + 30),
        "the picture came back after the owner was killed"
    );
    let gap = b.count.longest();
    println!("handover gap across processes: {gap:?}, took over in {} ms", b.share()["last_start_ms"]);
    assert_eq!(b.share()["owner"], true);
    assert_eq!(opens(&root), 2);
    assert!(gap < Duration::from_secs(3), "gap {gap:?}");
    b.close();
    loader::uninstall("sharebars").ok();
}
