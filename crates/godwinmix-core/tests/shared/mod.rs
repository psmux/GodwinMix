//! Helpers for the shared source tests: a plugin written into a directory,
//! a source opened the way the mixer opens one, and counters on its ends.

#![allow(dead_code)]

pub mod exit;

use godwinmix_core::config::{BrowserConfig, SourceConfig};
use godwinmix_core::plugin::source::{Source, SourceRequest};
use godwinmix_core::plugin::{harness, loader, Hello, MediaEnds, Tier, API_LEVEL};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub struct Fixture {
    pub manifest: &'static str,
    pub script: &'static str,
}

pub fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")
        .and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|p| p.is_file()))
}

/// Install the plugin into `root`, with this process's own runtime directory.
pub fn setup(root: &Path, f: &Fixture) {
    let checkout = root.join("checkout");
    std::fs::create_dir_all(&checkout).unwrap();
    std::fs::write(checkout.join("gmx-plugin.toml"), f.manifest).unwrap();
    std::fs::write(checkout.join("settings.json"), r#"{"type":"object","properties":{}}"#).unwrap();
    std::fs::write(checkout.join("run.sh"), f.script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(checkout.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    let plugins = root.join("plugins");
    std::fs::create_dir_all(&plugins).unwrap();
    loader::set_dir(plugins);
    loader::set_runtime_dir(root.join(format!("run-{}", std::process::id())));
    loader::install_from_path(&checkout).expect("it installs");
}

/// The bus directory and the open log, for this process and its children.
pub fn env_for(root: &Path) {
    std::env::set_var("GODWINMIX_BUS_DIR", root.join("bus"));
    std::env::set_var("SHAREBARS_LOG", root.join("opens.log"));
}

pub fn opens(root: &Path) -> usize {
    std::fs::read_to_string(root.join("opens.log")).map(|t| t.lines().count()).unwrap_or(0)
}

/// Buffers seen at one end, the longest gap between two, and the last pts.
#[derive(Default)]
pub struct Count {
    pub frames: AtomicU64,
    pub longest_us: AtomicU64,
    pub last_pts: AtomicU64,
    last: Mutex<Option<Instant>>,
}

impl Count {
    pub fn gap_reset(&self) {
        self.longest_us.store(0, Relaxed);
    }
    pub fn longest(&self) -> Duration {
        Duration::from_micros(self.longest_us.load(Relaxed))
    }
    pub fn frames(&self) -> u64 {
        self.frames.load(Relaxed)
    }

    fn install(self: &Arc<Self>, end: &gst::Element) {
        let c = self.clone();
        end.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            let now = Instant::now();
            if let Some(then) = c.last.lock().unwrap().replace(now) {
                c.longest_us.fetch_max(now.duration_since(then).as_micros() as u64, Relaxed);
            }
            if let Some(pts) = info.buffer().and_then(|b| b.pts()) {
                c.last_pts.store(pts.nseconds(), Relaxed);
            }
            c.frames.fetch_add(1, Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
}

pub struct Running {
    pub source: Box<dyn Source>,
    pub ends: MediaEnds,
    pub video: Arc<Count>,
    pub audio: Arc<Count>,
}

/// A source of `type_id` with `params`, started and playing.
pub fn open(type_id: &str, id: &str, params: &[(&str, &str)]) -> Running {
    let mut cfg = SourceConfig::bare(id, "");
    cfg.type_id = Some(type_id.into());
    for (k, v) in params {
        cfg.params.insert((*k).into(), toml::Value::String((*v).into()));
    }
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
    let hello = Hello {
        instance: id.into(),
        canvas: canvas.clone(),
        api_level: API_LEVEL,
        params: cfg.effective_params(),
        tier: Tier::Core,
    };
    source.initialize(hello).unwrap();
    let ends = source.start(&canvas, false).unwrap();
    let (video, audio) = (Arc::new(Count::default()), Arc::new(Count::default()));
    video.install(&ends.video);
    audio.install(&ends.audio);
    ends.pipeline.set_state(gst::State::Playing).unwrap();
    Running { source, ends, video, audio }
}

impl Running {
    pub fn frames(&self) -> u64 {
        self.video.frames()
    }
    pub fn share(&mut self) -> serde_json::Value {
        self.source.call("share", serde_json::json!({})).unwrap()
    }
    pub fn close(mut self) {
        let _ = self.ends.pipeline.set_state(gst::State::Null);
        self.source.stop().unwrap();
    }
}

/// Wait until `f` holds, for at most `limit`.
pub fn until(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
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
pub fn exclusive() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

pub fn skip() -> bool {
    if which("gst-launch-1.0").is_none() {
        println!("skipping: gst-launch-1.0 is not on PATH");
        return true;
    }
    false
}

pub fn temp(tag: &str) -> PathBuf {
    let path = PathBuf::from(format!("/tmp/gmx-share-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// This test binary started again as a second mixer, running `entry`.
pub fn spawn_child(entry: &str, root: &Path) -> (Child, BufReader<ChildStdout>) {
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", entry, "--nocapture", "--test-threads=1"])
        .env("SHARE_CHILD_ROOT", root)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let lines = BufReader::new(child.stdout.take().unwrap());
    (child, lines)
}

/// The next `RESULT` line a child printed.
pub fn result(lines: &mut BufReader<ChildStdout>) -> String {
    let mut line = String::new();
    loop {
        line.clear();
        assert!(lines.read_line(&mut line).unwrap() > 0, "the child said nothing");
        if let Some((_, rest)) = line.split_once("RESULT ") {
            return rest.trim().to_string();
        }
    }
}

pub fn kill9(child: &mut Child) {
    // SAFETY: a signal to our own child.
    unsafe { libc::kill(child.id() as i32, libc::SIGKILL) };
    let _ = child.wait();
}
