//! `gmx bench`: the footprint table, measured on the machine it is run on.
//!
//! Every performance claim GodwinMix makes has to come from a command anybody
//! can run, on a machine that is named, at a commit that is named. OBS
//! publishes no CPU or RAM figure at all and the public record is forum
//! anecdote; a developer sizing a box cannot plan against that. This prints a
//! table instead, writes it to `bench/results/<machine>-<date>.md`, and fails
//! the build when a row is over its target.
//!
//! # What it measures and how
//!
//! Each row runs the real thing, samples this process for a warm up and then a
//! steady window, and reports CPU as a fraction of one core and RSS in
//! megabytes. Rows that say "added" are a delta: the same process measured
//! twice, once with the thing and once without, in that order, in one run.
//!
//! CPU comes from `getrusage(RUSAGE_SELF)` on Unix, which counts every thread
//! of this process and no child. Resident memory comes from `/proc/self/statm`
//! on Linux and from `ps -o rss=` on macOS, because the current (not peak)
//! figure is what a delta needs and macOS has no `/proc`. Windows has neither,
//! so both come from one `powershell Get-Process` call; that path compiles and
//! runs everywhere PowerShell does, and says "unknown" where it cannot.
//!
//! # What it does not measure
//!
//! Rows that need scenes, a GPU or a reference board that is not this machine
//! are printed as "not yet" with their target, so the table has the same shape
//! on every machine and the gaps are visible rather than absent.

use godwinmix_core::config::{Config, MultiviewConfig, SnapshotConfig, SourceConfig};
use godwinmix_core::multiview::{MultiviewHandle, MultiviewRequest};
use godwinmix_core::snapshot::Tracker;
use anyhow::{Context, Result};
use clap::Args;
use gstreamer as gst;
use gstreamer::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The canvas every row is measured at. 720p30 is the default output preset
/// (09 section 4 item 13), so it is what the numbers should describe.
const BENCH_WIDTH: i32 = 1280;
const BENCH_HEIGHT: i32 = 720;
const BENCH_FPS: i32 = 30;

#[derive(Args, Debug, Clone)]
pub struct BenchArgs {
    /// Name of the machine these numbers belong to. Defaults to the hostname;
    /// use one of the reference machine ids (`pi4`, `pi5`, `n100`, `laptop`,
    /// `gpu`) when this is one of them, because the budgets are per machine.
    #[arg(long)]
    pub machine: Option<String>,

    /// Seconds of steady state to measure each row over.
    #[arg(long, default_value_t = 30)]
    pub seconds: u64,

    /// Seconds to let each row settle before measuring.
    #[arg(long, default_value_t = 5)]
    pub warmup: u64,

    /// Two seconds a row instead of thirty five. For working on the bench
    /// itself; the numbers are too noisy to publish.
    #[arg(long)]
    pub quick: bool,

    /// Print JSON instead of Markdown, for CI.
    #[arg(long)]
    pub json: bool,

    /// Exit non zero when a row is over the target for this machine.
    #[arg(long)]
    pub budget: bool,

    /// Run only the rows whose id contains this.
    #[arg(long)]
    pub only: Option<String>,

    /// Which encode path the mixer rows use: `auto` picks what this machine
    /// has, `software` pins the software encoder.
    ///
    /// Worth pinning because the encoder's idle cost is what `[program]
    /// encoder = "on-demand"` exists to remove, and on a machine with hardware
    /// encode that cost is small enough to hide in the noise. The number that
    /// matters for a Pi 5 or a laptop is the software one.
    #[arg(long, default_value = "auto")]
    pub encode: String,

    /// Where to write the Markdown table. Defaults to
    /// `bench/results/<machine>-<date>.md` under the working directory.
    #[arg(long)]
    pub out: Option<PathBuf>,

    /// Do not write a result file.
    #[arg(long)]
    pub no_write: bool,

    /// Where to keep the generated test clip. Defaults to the system temp
    /// directory, so a second run reuses it.
    #[arg(long)]
    pub clip: Option<PathBuf>,

    /// Internal: this process is the child of the cold start row. It builds a
    /// mixer, waits for the first encoded programme frame, prints how long
    /// that took and exits.
    /// The nightly run: every row over the full window, compared to the
    /// budgets in 09 section 3, written to `bench/results/<machine>-<date>.md`,
    /// and a non zero exit when a row is over.
    ///
    /// One flag rather than four, so the workflow that runs this every night
    /// and the person reproducing it by hand type the same thing.
    #[arg(long)]
    pub nightly: bool,
    #[arg(long, hide = true)]
    pub cold_start_child: bool,
}

impl BenchArgs {
    fn window(&self) -> Duration {
        Duration::from_secs(if self.quick { 2 } else { self.seconds.max(1) })
    }

    fn warmup(&self) -> Duration {
        Duration::from_secs(if self.quick { 1 } else { self.warmup })
    }

    /// What `--nightly` means, spelled once.
    ///
    /// The full window rather than the quick one, because a two second sample
    /// of a mixer is a sample of it starting up. Everything written down,
    /// because a nightly nobody kept is a nightly nobody can compare against.
    /// Non zero over budget, because that is the only part a workflow reads.
    fn nightly(mut self) -> Self {
        if self.nightly {
            self.quick = false;
            self.budget = true;
            self.no_write = false;
            self.only = None;
        }
        self
    }

    fn wants(&self, id: &str) -> bool {
        match &self.only {
            // Either way round, so `--only two-live` picks up `two-live-hw`
            // and `--only two-live-hw` still runs the group that contains it.
            Some(f) => id.contains(f.as_str()) || f.contains(id),
            None => true,
        }
    }
}

/// What one measurement window found.
#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Load {
    /// Fraction of one core: 0.5 is half a core, 2.0 is two cores busy.
    pub cores: f64,
    pub rss_mb: f64,
}

impl Load {
    fn minus(self, base: Load) -> Load {
        Load {
            cores: (self.cores - base.cores).max(0.0),
            rss_mb: (self.rss_mb - base.rss_mb).max(0.0),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Row {
    pub id: String,
    /// What the row means, in the words of the budget table.
    pub measure: String,
    pub target: String,
    pub cores: Option<f64>,
    pub rss_mb: Option<f64>,
    /// A number that is neither CPU nor memory: seconds for cold start,
    /// megabytes for the binary, frames per second for the mosaic.
    pub other: Option<String>,
    pub note: Option<String>,
    pub command: String,
    pub verdict: String,
}

impl Row {
    fn new(id: &str, measure: &str, target: &str, command: String) -> Self {
        Self {
            id: id.into(),
            measure: measure.into(),
            target: target.into(),
            cores: None,
            rss_mb: None,
            other: None,
            note: None,
            command,
            verdict: "measured".into(),
        }
    }

    fn load(mut self, l: Load) -> Self {
        self.cores = Some(l.cores);
        self.rss_mb = Some(l.rss_mb);
        self
    }

    fn other(mut self, v: String) -> Self {
        self.other = Some(v);
        self
    }

    fn note(mut self, v: &str) -> Self {
        self.note = Some(v.into());
        self
    }

    fn not_yet(id: &str, measure: &str, target: &str, why: &str) -> Self {
        let mut r = Self::new(id, measure, target, "not yet".into());
        r.verdict = "not yet".into();
        r.note = Some(why.into());
        r
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub machine: String,
    pub cpu: String,
    pub ram_gb: f64,
    pub os: String,
    pub gstreamer: String,
    pub commit: String,
    pub date: String,
    pub canvas: String,
    pub window_secs: u64,
    /// True when the bench itself was built without optimisation, which makes
    /// every CPU number meaningless. Published numbers come from a release
    /// build.
    pub debug_build: bool,
    pub rows: Vec<Row>,
    pub over_budget: Vec<String>,
}

// ---------------------------------------------------------------------------
// Sampling this process, one cfg gate per platform.
// ---------------------------------------------------------------------------

/// Seconds of CPU this process has used, over every thread, excluding
/// children. `None` when the platform has no way to say.
#[cfg(unix)]
fn cpu_seconds() -> Option<f64> {
    // getrusage is POSIX and in libc on both Linux and macOS, so one path
    // covers every machine GodwinMix targets except Windows.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    let secs = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1_000_000.0;
    Some(secs(usage.ru_utime) + secs(usage.ru_stime))
}

#[cfg(not(unix))]
fn cpu_seconds() -> Option<f64> {
    windows_process().map(|(cpu, _)| cpu)
}

/// Resident set in bytes, now rather than at its peak, because the deltas need
/// a current figure.
#[cfg(target_os = "linux")]
fn rss_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as u64;
    Some(pages * page)
}

#[cfg(target_os = "macos")]
fn rss_bytes() -> Option<u64> {
    // macOS has no /proc and libc carries no current RSS call, so this is the
    // documented fallback: one `ps` per sample, a handful of times per row.
    let pid = std::process::id().to_string();
    let out = std::process::Command::new("ps").args(["-o", "rss=", "-p", &pid]).output().ok()?;
    let kb: u64 = String::from_utf8_lossy(&out.stdout).trim().parse().ok()?;
    Some(kb * 1024)
}

#[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
fn rss_bytes() -> Option<u64> {
    // Another Unix: peak rather than current, which is the honest thing to
    // report when the current figure is not available.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut usage) } != 0 {
        return None;
    }
    Some(usage.ru_maxrss as u64 * 1024)
}

#[cfg(not(unix))]
fn rss_bytes() -> Option<u64> {
    windows_process().map(|(_, rss)| rss)
}

/// CPU seconds and working set on Windows, where there is no getrusage and no
/// /proc. One PowerShell call rather than a dependency on `windows-sys`.
#[cfg(not(unix))]
fn windows_process() -> Option<(f64, u64)> {
    let pid = std::process::id().to_string();
    let script = format!(
        "$p = Get-Process -Id {pid}; \
         Write-Output ($p.TotalProcessorTime.TotalSeconds.ToString() + ' ' + $p.WorkingSet64)"
    );
    let out = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let mut parts = text.split_whitespace();
    let cpu: f64 = parts.next()?.parse().ok()?;
    let rss: u64 = parts.next()?.parse().ok()?;
    Some((cpu, rss))
}

fn rss_mb() -> f64 {
    rss_bytes().map(|b| b as f64 / 1_048_576.0).unwrap_or(0.0)
}

/// Let the thing settle, then watch it for a window. RSS is the highest of a
/// few readings across the window, so one unlucky moment does not decide it.
async fn steady(warmup: Duration, window: Duration) -> Load {
    tokio::time::sleep(warmup).await;
    let c0 = cpu_seconds().unwrap_or(0.0);
    let mut peak = rss_mb();
    let steps = 6u32;
    for _ in 0..steps {
        tokio::time::sleep(window / steps).await;
        peak = peak.max(rss_mb());
    }
    let c1 = cpu_seconds().unwrap_or(0.0);
    Load { cores: (c1 - c0) / window.as_secs_f64(), rss_mb: peak }
}

// ---------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------

fn shell(cmd: &str, args: &[&str]) -> Option<String> {
    let out = std::process::Command::new(cmd).args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

fn hostname() -> String {
    #[cfg(unix)]
    {
        let mut buf = [0i8; 256];
        // SAFETY: the buffer and its length are handed over together and the
        // result is read only as far as its first NUL.
        if unsafe { libc::gethostname(buf.as_mut_ptr(), buf.len()) } == 0 {
            let bytes: Vec<u8> =
                buf.iter().take_while(|c| **c != 0).map(|c| *c as u8).collect();
            if let Ok(name) = String::from_utf8(bytes) {
                return name.split('.').next().unwrap_or("host").to_string();
            }
        }
    }
    std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| "host".into())
}

fn cpu_model() -> String {
    #[cfg(target_os = "macos")]
    if let Some(m) = shell("sysctl", &["-n", "machdep.cpu.brand_string"]) {
        return m;
    }
    #[cfg(target_os = "linux")]
    if let Ok(info) = std::fs::read_to_string("/proc/cpuinfo") {
        for line in info.lines() {
            if let Some(v) = line.split_once(':') {
                if matches!(v.0.trim(), "model name" | "Model" | "Hardware") {
                    return v.1.trim().to_string();
                }
            }
        }
    }
    shell("wmic", &["cpu", "get", "name"])
        .and_then(|s| s.lines().nth(1).map(|l| l.trim().to_string()))
        .unwrap_or_else(|| std::env::consts::ARCH.to_string())
}

fn ram_gb() -> f64 {
    #[cfg(target_os = "macos")]
    if let Some(b) = shell("sysctl", &["-n", "hw.memsize"]).and_then(|s| s.parse::<u64>().ok()) {
        return b as f64 / 1_073_741_824.0;
    }
    #[cfg(target_os = "linux")]
    if let Ok(info) = std::fs::read_to_string("/proc/meminfo") {
        for line in info.lines() {
            if let Some(rest) = line.strip_prefix("MemTotal:") {
                if let Some(kb) = rest.split_whitespace().next().and_then(|v| v.parse::<u64>().ok())
                {
                    return kb as f64 / 1_048_576.0;
                }
            }
        }
    }
    0.0
}

fn os_name() -> String {
    let release = shell("uname", &["-r"]).unwrap_or_default();
    format!("{} {} {}", std::env::consts::OS, std::env::consts::ARCH, release)
        .trim()
        .to_string()
}

fn commit() -> String {
    shell("git", &["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".into())
}

/// Today, as `YYYY-MM-DD`, without pulling in a date crate for one line.
fn today() -> String {
    shell("date", &["+%Y-%m-%d"])
        .or_else(|| shell("powershell", &["-NoProfile", "-Command", "Get-Date -Format yyyy-MM-dd"]))
        .unwrap_or_else(|| "unknown-date".into())
}

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A 720p30 H.264 clip to decode, written once and reused. Long enough that a
/// warm up and a full window fit inside it without an end of stream.
fn test_clip(args: &BenchArgs) -> Result<PathBuf> {
    let path = args
        .clip
        .clone()
        .unwrap_or_else(|| std::env::temp_dir().join("gmx-bench-720p30.mp4"));
    let need = args.warmup().as_secs() + args.window().as_secs() + 10;
    if path.exists() {
        return Ok(path);
    }
    let frames = need * BENCH_FPS as u64;
    let desc = format!(
        "videotestsrc num-buffers={frames} pattern=smpte ! \
         video/x-raw,width={BENCH_WIDTH},height={BENCH_HEIGHT},framerate={BENCH_FPS}/1 ! \
         x264enc speed-preset=veryfast bitrate=2500 key-int-max={BENCH_FPS} ! h264parse ! \
         mp4mux ! filesink location={}",
        path.display()
    );
    run_pipeline_to_end(&desc).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// Play a pipeline description through to end of stream.
fn run_pipeline_to_end(desc: &str) -> Result<()> {
    let pipeline = gst::parse::launch(desc).context("parsing pipeline")?;
    pipeline.set_state(gst::State::Playing)?;
    let bus = pipeline.bus().context("pipeline has no bus")?;
    for msg in bus.iter_timed(gst::ClockTime::from_seconds(600)) {
        match msg.view() {
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(e) => {
                let _ = pipeline.set_state(gst::State::Null);
                anyhow::bail!("{}: {}", e.error(), e.debug().unwrap_or_default());
            }
            _ => {}
        }
    }
    pipeline.set_state(gst::State::Null)?;
    Ok(())
}

/// Start a pipeline description, measure it, stop it. The RSS is the whole
/// process, which is only meaningful against another reading from the same
/// run; `measure_pipeline_added` is the one to use for a number on its own.
async fn measure_pipeline(args: &BenchArgs, desc: &str) -> Result<Load> {
    let pipeline = gst::parse::launch(desc).with_context(|| format!("parsing {desc}"))?;
    pipeline.set_state(gst::State::Playing).context("starting the pipeline")?;
    let load = steady(args.warmup(), args.window()).await;
    pipeline.set_state(gst::State::Null).ok();
    Ok(load)
}

/// The same, with the resident memory this process was already holding taken
/// off, so the RSS column is what the pipeline added rather than everything
/// the bench has allocated since it started.
async fn measure_pipeline_added(args: &BenchArgs, desc: &str) -> Result<Load> {
    let floor = rss_mb();
    let load = measure_pipeline(args, desc).await?;
    Ok(load.minus(Load { cores: 0.0, rss_mb: floor }))
}

/// The idle core with `[program] encoder = "always"`: what a core cost before
/// the encoder learned to wait for a consumer.
///
/// Its own mixer, because the policy is read once in `Mixer::build`. Torn down
/// before it returns, so the rows after it start from a clean process.
async fn encoder_always_row(args: &BenchArgs) -> Result<Row> {
    let mut cfg = bench_config_with(
        MultiviewConfig { enabled: false, ..Default::default() },
        &args.encode,
    );
    cfg.program.encoder = "always".into();
    let (mut mix, handle, cmd_rx, _bus_rx) = godwinmix_core::mixer::Mixer::build(cfg)?;
    mix.start().context("starting the always-on encoder mixer")?;
    let encoder = mix.encoder_handle();
    let thread = godwinmix_core::mixer::spawn(mix, cmd_rx, handle.clone());
    let load = steady(args.warmup(), args.window()).await;
    let running = encoder.is_running();
    let _ = handle.send(godwinmix_core::mixer::Command::Shutdown);
    let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    Ok(Row::new(
        "core-idle-encoder-always",
        "Core idle with [program] encoder = \"always\", no outputs, nobody subscribed",
        "documented, not budgeted: it is what the default avoids",
        "gmx bench --only core-idle-encoder-always".into(),
    )
    .load(load)
    .other(format!("encoder running: {running}"))
    .note(
        "What every release before this one did: the encode chain built, linked \
         and encoding a black slate from boot with nothing attached to read it. \
         The difference against `core-idle` above is what `on-demand` saves, and \
         it is largest where the encoder is software.",
    ))
}

/// The config every mixer row is built from: 720p30, no outputs, no sources.
fn bench_config(multiview: MultiviewConfig) -> Config {
    bench_config_with(multiview, "auto")
}

/// The same, with the encode path pinned. `--encode software` is what makes
/// the encoder rows mean anything on a machine with hardware encode.
fn bench_config_with(multiview: MultiviewConfig, encode: &str) -> Config {
    let mut cfg: Config = toml::from_str("").expect("an empty config is every default");
    if encode == "software" {
        cfg.hardware.encode = godwinmix_core::config::Accel::Software;
    }
    cfg.canvas = godwinmix_core::config::Canvas {
        width: BENCH_WIDTH,
        height: BENCH_HEIGHT,
        fps: BENCH_FPS,
        sample_rate: 48000,
        channels: 2,
    };
    cfg.program.video_bitrate_kbps = 2500;
    cfg.multiview = multiview;
    cfg
}

// ---------------------------------------------------------------------------
// The rows
// ---------------------------------------------------------------------------

/// Everything that needs a running mixer, measured against one: the core at
/// rest, the mosaic with nobody watching, the mosaic with one subscriber, and
/// the snapshot tracker on top of that.
async fn mixer_rows(args: &BenchArgs) -> Result<Vec<Row>> {
    let mut rows = Vec::new();
    let cfg = bench_config_with(
        MultiviewConfig { width: 1280, height: 720, fps: 8, ..Default::default() },
        &args.encode,
    );
    let (mut mix, handle, cmd_rx, _bus_rx) = godwinmix_core::mixer::Mixer::build(cfg)?;
    mix.start().context("starting the mixer")?;
    let mv: MultiviewHandle = mix.multiview_handle();
    let thread = godwinmix_core::mixer::spawn(mix, cmd_rx, handle.clone());

    let idle = steady(args.warmup(), args.window()).await;
    rows.push(
        Row::new(
            "core-idle",
            "Core idle RSS and CPU, no sources, no outputs, nobody subscribed",
            "at most 60 MB, at most 1 percent of one core on `pi4`",
            "gmx bench --only core-idle".into(),
        )
        .load(idle)
        .note(
            "With `[program] encoder = \"on-demand\"`, which is the default, the \
             encode chain is off the raw tees and at NULL until something reads \
             it. The compositor, the audio mixer and both raw tees run, so the \
             picture never stops and a take is never delayed. The row below is \
             the same core with the encoder pinned on, which is what every \
             release before this one did.",
        ),
    );

    // The same idle core with the encoder pinned on, so the table carries the
    // cost of the old behaviour beside the new one rather than asking a reader
    // to take the saving on trust. A second mixer, because the policy is read
    // once at build time.
    rows.push(encoder_always_row(args).await?);

    // With multiview enabled in the config and nobody subscribed there is no
    // mosaic pipeline at all, which is the whole point of the row.
    debug_assert_eq!(mv.live_pipelines(), 0);
    rows.push(
        Row::new(
            "multiview-idle",
            "Multiview encoder with no client subscribed",
            "0, and no pipeline",
            "gmx bench --only multiview".into(),
        )
        .load(Load::default())
        .other(format!("{} mosaic pipelines alive", mv.live_pipelines()))
        .note("Asserted, not sampled: there is no mosaic object to cost anything."),
    );

    let sub = mv.subscribe(MultiviewRequest { fps: 8, width: 1280 });
    // Counted here rather than read off the handle's metric, because a bench
    // that trusts the number it is meant to be checking proves nothing.
    let counted = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let counting = {
        let mut sub = mv.subscribe(MultiviewRequest { fps: 8, width: 1280 });
        let counted = counted.clone();
        let since = Instant::now();
        tokio::spawn(async move {
            while sub.recv().await.is_ok() {
                counted.fetch_add(1, Ordering::Relaxed);
            }
            since
        })
    };
    let started = Instant::now();
    let with_mosaic = steady(args.warmup(), args.window()).await;
    let rate = counted.load(Ordering::Relaxed) as f64 / started.elapsed().as_secs_f64();
    counting.abort();
    rows.push(
        Row::new(
            "multiview-subscriber",
            "Multiview encoder, one subscriber, 8 fps mosaic 1280 wide",
            "at most 10 percent of one core on `pi5`",
            "gmx bench --only multiview-subscriber".into(),
        )
        .load(with_mosaic.minus(idle))
        .other(format!("{rate:.1} fps out, {:.1} fps reported", mv.fps())),
    );

    // The tracker decodes a mosaic frame per tick and scores it. Something has
    // to keep asking, exactly as an agent polling `agent.state` would.
    let tracker = Tracker::new(
        SnapshotConfig { idle_secs: 2, ..Default::default() },
        mv.clone(),
        handle.clone(),
    );
    let asking = Arc::new(AtomicBool::new(true));
    {
        let tracker = tracker.clone();
        let asking = asking.clone();
        tokio::spawn(async move {
            while asking.load(Ordering::Relaxed) {
                tracker.want();
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
    }
    let with_tracker = steady(args.warmup(), args.window()).await;
    asking.store(false, Ordering::Relaxed);
    rows.push(
        Row::new(
            "snapshot-tracker",
            "Snapshot and motion tracker, while something is asking",
            "at most 5 percent of one core on `pi5`, and 0 when disabled",
            "gmx bench --only snapshot".into(),
        )
        .load(with_tracker.minus(with_mosaic))
        .other(format!(
            "{:.3} cores for the mosaic and the tracker together, tracker started {} time(s)",
            with_tracker.cores,
            tracker.starts()
        ))
        .note(
            "The delta is against the mosaic on its own, so on a fast machine it \
             can disappear into the noise between two windows. Zero when nothing \
             asks is not a rounding: the follower is not running at all then.",
        ),
    );

    drop(sub);
    let _ = handle.send(godwinmix_core::mixer::Command::Shutdown);
    tokio::task::spawn_blocking(move || thread.join()).await.ok();
    Ok(rows)
}

/// One 720p30 file decoded into a compositor with nothing encoding on the far
/// side, measured as the difference between a compositor at rest and the same
/// compositor with the file in it.
async fn file_source_row(args: &BenchArgs, clip: &Path) -> Result<Row> {
    let caps = format!("video/x-raw,width={BENCH_WIDTH},height={BENCH_HEIGHT},framerate={BENCH_FPS}/1");
    let base = format!(
        "videotestsrc pattern=black is-live=true ! {caps} ! \
         compositor name=comp background=black ! {caps} ! fakesink sync=true"
    );
    let with_file = format!(
        "{base} filesrc location={} ! decodebin ! videoconvert ! videoscale ! \
         videorate ! {caps} ! comp.",
        clip.display()
    );
    let baseline = measure_pipeline(args, &base).await?;
    let loaded = measure_pipeline(args, &with_file).await?;
    let hw_decode = godwinmix_core::probe::Backends::probe(
        godwinmix_core::config::Accel::Auto,
        godwinmix_core::config::Accel::Auto,
    )
    .map(|b| b.video_decode.accel != godwinmix_core::config::Accel::Software)
    .unwrap_or(false);
    let mut note = String::from(
        "In process, not container mode: a sidecar source pays about 41 MB more \
         for its own GStreamer process.",
    );
    if hw_decode {
        note.push_str(
            " This machine decodes in hardware, and on macOS VideoToolbox runs in \
             its own XPC process, so the decode does not appear in this process's \
             CPU at all. The number is the mixer's own cost, not the machine's.",
        );
    }
    Ok(Row::new(
        "file-source",
        "Added RSS and CPU per 720p30 file source into the compositor, no encode",
        "at most 40 MB, at most 15 percent of one core on `pi4`",
        format!("gst-launch-1.0 {with_file}"),
    )
    .load(loaded.minus(baseline))
    .note(&note))
}

/// Two live 720p30 sources composited and encoded, once on x264 and once on
/// whatever the codec probe says this machine has.
async fn programme_rows(args: &BenchArgs) -> Result<Vec<Row>> {
    let caps = format!("video/x-raw,width={BENCH_WIDTH},height={BENCH_HEIGHT},framerate={BENCH_FPS}/1");
    let sources = format!(
        "videotestsrc pattern=smpte is-live=true ! {caps} ! comp.sink_0 \
         videotestsrc pattern=ball is-live=true ! {caps} ! comp.sink_1"
    );
    let mut rows = Vec::new();

    let sw = format!(
        "compositor name=comp background=black ! {caps} ! videoconvert ! \
         x264enc speed-preset=veryfast bitrate=2500 key-int-max={} ! h264parse ! \
         fakesink sync=true {sources}",
        BENCH_FPS * 2
    );
    rows.push(
        Row::new(
            "two-live-sw",
            "720p30, two live sources, programme encode, software x264",
            "`pi5` software: at most 2.0 cores; `laptop`: documented",
            format!("gst-launch-1.0 {sw}"),
        )
        .load(measure_pipeline_added(args, &sw).await?)
        .note("RSS is what this pipeline added to the process, not the process total."),
    );

    let backends = godwinmix_core::probe::Backends::probe(
        godwinmix_core::config::Accel::Auto,
        godwinmix_core::config::Accel::Auto,
    )?;
    let hw = backends.video_encode.element;
    if backends.video_encode.accel == godwinmix_core::config::Accel::Software {
        rows.push(Row::not_yet(
            "two-live-hw",
            "720p30, two live sources, programme encode, hardware",
            "`pi4` with hardware encode: at most 1.0 core total; `n100`: at most 0.6",
            "no hardware encoder on this machine: the codec probe chose software",
        ));
        return Ok(rows);
    }
    let hw_pipeline = format!(
        "compositor name=comp background=black ! {caps} ! videoconvert ! {hw} ! \
         h264parse ! fakesink sync=true {sources}"
    );
    match measure_pipeline_added(args, &hw_pipeline).await {
        Ok(load) => rows.push(
            Row::new(
                "two-live-hw",
                "720p30, two live sources, programme encode, hardware",
                "`pi4` with hardware encode: at most 1.0 core total; `n100`: at most 0.6",
                format!("gst-launch-1.0 {hw_pipeline}"),
            )
            .load(load)
            .other(format!("encoder {hw}"))
            .note("RSS is what this pipeline added to the process, not the process total."),
        ),
        Err(e) => rows.push(Row::not_yet(
            "two-live-hw",
            "720p30, two live sources, programme encode, hardware",
            "`pi4` with hardware encode: at most 1.0 core total; `n100`: at most 0.6",
            &format!("{hw} would not run here: {e:#}"),
        )),
    }
    Ok(rows)
}

/// Process exec to first encoded programme frame, timed by the parent against
/// a child that prints the moment the encoder produces a buffer.
async fn cold_start_row(clip: &Path) -> Result<Row> {
    let exe = std::env::current_exe().context("finding this binary")?;
    let started = Instant::now();
    let out = std::process::Command::new(&exe)
        .args(["bench", "--cold-start-child", "--clip"])
        .arg(clip)
        .output()
        .context("running the cold start child")?;
    let wall = started.elapsed();
    let text = String::from_utf8_lossy(&out.stdout);
    let child_ms: Option<f64> = text
        .lines()
        .find_map(|l| l.strip_prefix("cold-start-ms "))
        .and_then(|v| v.trim().parse().ok());
    if child_ms.is_none() {
        return Ok(Row::not_yet(
            "cold-start",
            "Cold start, process exec to first encoded programme frame",
            "at most 2.0 s on `pi4`",
            &format!("the child said nothing useful: {}", String::from_utf8_lossy(&out.stderr)),
        ));
    }
    Ok(Row::new(
        "cold-start",
        "Cold start, process exec to first encoded programme frame",
        "at most 2.0 s on `pi4`",
        "gmx bench --only cold-start".into(),
    )
    .other(format!("{:.2} s", wall.as_secs_f64()))
    .note(&format!(
        "Wall clock from exec in the parent. The child's own clock, from the \
         first line of its main to the first encoded frame, read {:.0} ms; the \
         difference is process start and dynamic linking.",
        child_ms.unwrap_or_default()
    )))
}

/// The child half of the cold start row: build the mixer the way `run` does,
/// watch the encoder's src pad, print and exit.
pub async fn cold_start_child(args: &BenchArgs) -> Result<()> {
    let started = Instant::now();
    let clip = args.clip.clone().context("the cold start child needs --clip")?;
    let mut cfg = bench_config(MultiviewConfig { enabled: false, ..Default::default() });
    // Built the way the API builds one, so the child starts the same pipeline
    // a real boot would.
    let source: SourceConfig = serde_json::from_value(serde_json::json!({
        "id": "clip",
        "uri": format!("file://{}", clip.display()),
    }))?;
    cfg.sources = vec![source];
    let (mut mix, handle, cmd_rx, _bus_rx) = godwinmix_core::mixer::Mixer::build(cfg)?;
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();
    let venc = mix
        .program_pipeline()
        .by_name("venc")
        .context("the programme pipeline has no element called venc")?;
    let pad = venc.static_pad("src").context("venc has no src pad")?;
    let tx = std::sync::Mutex::new(Some(tx));
    pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        if let Some(tx) = tx.lock().ok().and_then(|mut t| t.take()) {
            let _ = tx.send(());
        }
        gst::PadProbeReturn::Remove
    });
    mix.start()?;
    let thread = godwinmix_core::mixer::spawn(mix, cmd_rx, handle.clone());
    let waited = tokio::time::timeout(Duration::from_secs(30), rx).await;
    println!("cold-start-ms {:.0}", started.elapsed().as_secs_f64() * 1000.0);
    let _ = handle.send(godwinmix_core::mixer::Command::Shutdown);
    let _ = tokio::task::spawn_blocking(move || thread.join()).await;
    waited.context("no encoded frame within thirty seconds")?.ok();
    Ok(())
}

/// The release binary, which is what a user downloads.
fn binary_row() -> Row {
    let candidates = [
        PathBuf::from("target/release/gmx"),
        PathBuf::from("target/release/godwinmix"),
        std::env::current_exe().unwrap_or_default(),
    ];
    let found = candidates.iter().find(|p| p.exists()).cloned();
    let row = Row::new(
        "binary-size",
        "Core binary, one executable, before the platform's GStreamer packages",
        "at most 30 MB, plus about 19 MB of GStreamer",
        "ls -l target/release/gmx".into(),
    );
    match found.and_then(|p| std::fs::metadata(&p).ok().map(|m| (p, m.len()))) {
        Some((path, len)) => {
            let mb = len as f64 / 1_048_576.0;
            let debug = !path.to_string_lossy().contains("release");
            row.other(format!("{mb:.1} MB")).note(if debug {
                "This is a debug build. Run `cargo build --release` and bench that."
            } else {
                "Release build, unstripped."
            })
        }
        None => Row::not_yet(
            "binary-size",
            "Core binary",
            "at most 30 MB",
            "no built binary found next to the working directory",
        ),
    }
}

fn scene_rows() -> Vec<Row> {
    vec![
        Row::not_yet(
            "crossfade-1080p",
            "A crossfade between two eight item scenes at 1080p30",
            "`gpu` and `n100`: no dropped frame; `laptop`: documented",
            "scenes do not exist yet, so there is nothing to fade between",
        ),
        Row::not_yet(
            "hidden-slots",
            "Sixteen hidden slots at alpha 0",
            "within 2 percent of the no compositor baseline",
            "scenes do not exist yet",
        ),
    ]
}

// ---------------------------------------------------------------------------
// Budgets
// ---------------------------------------------------------------------------

/// A budget from 09 section 3. `machine` is `None` when the target does not
/// depend on the machine, and then it is checked everywhere.
struct Budget {
    row: &'static str,
    machine: Option<&'static str>,
    metric: Metric,
    limit: f64,
}

enum Metric {
    Cores,
    RssMb,
    /// The `other` column, parsed as a leading number.
    Other,
}

const BUDGETS: &[Budget] = &[
    Budget { row: "core-idle", machine: None, metric: Metric::RssMb, limit: 60.0 },
    Budget { row: "core-idle", machine: Some("pi4"), metric: Metric::Cores, limit: 0.01 },
    Budget { row: "file-source", machine: None, metric: Metric::RssMb, limit: 40.0 },
    Budget { row: "file-source", machine: Some("pi4"), metric: Metric::Cores, limit: 0.15 },
    // Not "about zero": zero. There is no pipeline to cost anything.
    Budget { row: "multiview-idle", machine: None, metric: Metric::Cores, limit: 0.0 },
    Budget { row: "multiview-subscriber", machine: Some("pi5"), metric: Metric::Cores, limit: 0.10 },
    Budget { row: "snapshot-tracker", machine: Some("pi5"), metric: Metric::Cores, limit: 0.05 },
    Budget { row: "two-live-sw", machine: Some("pi5"), metric: Metric::Cores, limit: 2.0 },
    Budget { row: "two-live-hw", machine: Some("pi4"), metric: Metric::Cores, limit: 1.0 },
    Budget { row: "two-live-hw", machine: Some("n100"), metric: Metric::Cores, limit: 0.6 },
    Budget { row: "cold-start", machine: Some("pi4"), metric: Metric::Other, limit: 2.0 },
    Budget { row: "binary-size", machine: None, metric: Metric::Other, limit: 30.0 },
];

fn check_budgets(machine: &str, rows: &mut [Row]) -> Vec<String> {
    let mut over = Vec::new();
    for b in BUDGETS {
        if b.machine.is_some_and(|m| m != machine) {
            continue;
        }
        let Some(row) = rows.iter_mut().find(|r| r.id == b.row) else { continue };
        if row.verdict == "not yet" {
            continue;
        }
        let (got, unit) = match b.metric {
            Metric::Cores => (row.cores, "cores"),
            Metric::RssMb => (row.rss_mb, "MB"),
            Metric::Other => (
                row.other
                    .as_ref()
                    .and_then(|o| o.split_whitespace().next())
                    .and_then(|v| v.parse().ok()),
                "",
            ),
        };
        let Some(got) = got else { continue };
        // A hair over a target is measurement noise, not a regression, except
        // where the target is zero and any number at all is a bug.
        let slack = if b.limit == 0.0 { 0.0 } else { b.limit * 0.02 };
        if got > b.limit + slack {
            row.verdict = format!("over ({got:.3} {unit} against {:.3})", b.limit);
            over.push(format!("{}: {got:.3} {unit}, budget {:.3}", row.id, b.limit));
        } else {
            row.verdict = "within budget".into();
        }
    }
    over
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn cell(v: Option<f64>, fmt: impl Fn(f64) -> String) -> String {
    v.map(fmt).unwrap_or_else(|| "-".into())
}

pub fn render_markdown(r: &Report) -> String {
    let mut s = String::new();
    s.push_str(&format!("# Footprint on `{}`\n\n", r.machine));
    s.push_str(&format!(
        "| | |\n|---|---|\n| Machine | `{}` |\n| CPU | {} |\n| RAM | {:.0} GB |\n\
         | OS | {} |\n| GStreamer | {} |\n| Commit | `{}` |\n| Date | {} |\n\
         | Canvas | {} |\n| Window | {} s steady state after warm up |\n\n",
        r.machine, r.cpu, r.ram_gb, r.os, r.gstreamer, r.commit, r.date, r.canvas, r.window_secs
    ));
    if r.debug_build {
        s.push_str(
            "> Built without optimisation. Every CPU number below is wrong by a \
             large factor. Run `cargo build --release` and bench that binary.\n\n",
        );
    }
    s.push_str("| Measure | Target | CPU (cores) | RSS (MB) | Other | Verdict |\n");
    s.push_str("|---|---|---|---|---|---|\n");
    for row in &r.rows {
        s.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            row.measure,
            row.target,
            cell(row.cores, |v| format!("{v:.3}")),
            cell(row.rss_mb, |v| format!("{v:.1}")),
            row.other.clone().unwrap_or_else(|| "-".into()),
            row.verdict,
        ));
    }
    s.push_str(
        "\nCPU is a fraction of one core over the window. RSS on a row that says \
         \"added\" is a difference between two readings in one process, taken in \
         the order the rows are printed, so a later row starts from whatever an \
         earlier one did not give back to the operating system. For a clean \
         figure on one row, run it on its own with `--only`.\n",
    );
    s.push_str("\n## What produced each row\n\n");
    for row in &r.rows {
        s.push_str(&format!("### `{}`\n\n", row.id));
        if let Some(n) = &row.note {
            s.push_str(&format!("{n}\n\n"));
        }
        s.push_str(&format!("```\n{}\n```\n\n", row.command));
    }
    if !r.over_budget.is_empty() {
        s.push_str("## Over budget\n\n");
        for line in &r.over_budget {
            s.push_str(&format!("* {line}\n"));
        }
        s.push('\n');
    }
    s
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

pub async fn run(args: BenchArgs) -> Result<()> {
    if args.cold_start_child {
        return cold_start_child(&args).await;
    }
    let args = args.nightly();
    let machine = args.machine.clone().unwrap_or_else(hostname);
    let mut rows: Vec<Row> = Vec::new();

    // The mixer rows go first, while the process has allocated as little as
    // possible: resident memory is sticky, so a later row would flatter itself
    // with an arena somebody else grew.
    if args.wants("core-idle")
        || args.wants("multiview")
        || args.wants("snapshot")
    {
        rows.extend(mixer_rows(&args).await?);
        rows.retain(|r| args.wants(&r.id));
    }

    let clip_needed = args.wants("file-source") || args.wants("cold-start");
    let clip = if clip_needed { Some(test_clip(&args)?) } else { None };

    if args.wants("file-source") {
        rows.push(file_source_row(&args, clip.as_ref().unwrap()).await?);
    }
    if args.wants("two-live") {
        rows.extend(programme_rows(&args).await?);
    }
    if args.wants("cold-start") {
        rows.push(cold_start_row(clip.as_ref().unwrap()).await?);
    }
    if args.wants("binary-size") {
        rows.push(binary_row());
    }
    for row in scene_rows() {
        if args.wants(&row.id) {
            rows.push(row);
        }
    }

    let over = check_budgets(&machine, &mut rows);
    let report = Report {
        machine: machine.clone(),
        cpu: cpu_model(),
        ram_gb: ram_gb(),
        os: os_name(),
        gstreamer: gst::version_string().to_string(),
        commit: commit(),
        date: today(),
        canvas: format!("{BENCH_WIDTH}x{BENCH_HEIGHT} at {BENCH_FPS} fps"),
        window_secs: args.window().as_secs(),
        debug_build: cfg!(debug_assertions),
        rows,
        over_budget: over.clone(),
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        print!("{}", render_markdown(&report));
    }

    if !args.no_write {
        let path = args.out.clone().unwrap_or_else(|| {
            PathBuf::from("bench/results").join(format!("{}-{}.md", report.machine, report.date))
        });
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        std::fs::write(&path, render_markdown(&report))
            .with_context(|| format!("writing {}", path.display()))?;
        eprintln!("wrote {}", path.display());
    }

    if args.budget && !over.is_empty() {
        anyhow::bail!(
            "{} row(s) over budget on {machine}: {}",
            over.len(),
            over.join("; ")
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_can_be_sampled_on_this_platform() {
        let cpu = cpu_seconds();
        assert!(cpu.is_some(), "no CPU figure on {}", std::env::consts::OS);
        assert!(cpu.unwrap() >= 0.0);
        let rss = rss_bytes().expect("no RSS figure");
        assert!(rss > 1_000_000, "{rss} bytes is not a running process");
    }

    #[test]
    fn a_zero_target_has_no_slack_and_a_real_one_does() {
        let row = |id: &str, cores: f64| Row::new(id, "", "", String::new()).load(Load {
            cores,
            rss_mb: 1.0,
        });
        // The mosaic with nobody watching must be exactly nothing.
        let mut rows = vec![row("multiview-idle", 0.0001)];
        assert_eq!(check_budgets("laptop", &mut rows).len(), 1);
        let mut rows = vec![row("multiview-idle", 0.0)];
        assert!(check_budgets("laptop", &mut rows).is_empty());
        assert_eq!(rows[0].verdict, "within budget");

        // A machine specific budget is not applied to another machine.
        let mut rows = vec![row("snapshot-tracker", 0.9)];
        assert!(check_budgets("laptop", &mut rows).is_empty());
        let mut rows = vec![row("snapshot-tracker", 0.9)];
        assert_eq!(check_budgets("pi5", &mut rows).len(), 1);
        assert!(rows[0].verdict.starts_with("over"), "{}", rows[0].verdict);
    }

    #[test]
    fn a_row_that_could_not_be_measured_is_never_over_budget() {
        let mut rows = vec![Row::not_yet("cold-start", "", "", "no hardware")];
        assert!(check_budgets("pi4", &mut rows).is_empty());
        assert_eq!(rows[0].verdict, "not yet");
    }

    #[test]
    fn the_table_names_the_machine_the_commit_and_the_command() {
        let report = Report {
            machine: "m4pro".into(),
            cpu: "Apple M4 Pro".into(),
            ram_gb: 24.0,
            os: "macos aarch64".into(),
            gstreamer: "GStreamer 1.28.7".into(),
            commit: "abc1234".into(),
            date: "2026-09-14".into(),
            canvas: "1280x720 at 30 fps".into(),
            window_secs: 30,
            debug_build: false,
            rows: vec![Row::new("core-idle", "Core idle", "60 MB", "gmx bench".into())
                .load(Load { cores: 0.02, rss_mb: 55.5 })],
            over_budget: vec![],
        };
        let md = render_markdown(&report);
        assert!(md.contains("m4pro"));
        assert!(md.contains("abc1234"));
        assert!(md.contains("GStreamer 1.28.7"));
        assert!(md.contains("0.020"));
        assert!(md.contains("55.5"));
        assert!(md.contains("gmx bench"), "the command that produced the row is missing");
    }

    #[test]
    fn a_delta_never_goes_negative() {
        let big = Load { cores: 1.0, rss_mb: 100.0 };
        let small = Load { cores: 0.2, rss_mb: 40.0 };
        assert_eq!(big.minus(small).cores, 0.8);
        // Noise can make the second reading the smaller one; that is a zero,
        // not a negative cost.
        assert_eq!(small.minus(big).cores, 0.0);
        assert_eq!(small.minus(big).rss_mb, 0.0);
    }
}
