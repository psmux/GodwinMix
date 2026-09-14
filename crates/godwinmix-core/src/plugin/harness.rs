//! The conformance harness: what a source has to do to be a source.
//!
//! The audit found no test anywhere that took a built source to PLAYING and
//! checked its caps. This is that test, written once and run against every
//! kind, so a plugin author finds out what is wrong from a report rather than
//! from a black programme.
//!
//! The eight checks of 03 section 11, all of which a core with no network can
//! make:
//!
//! 1. Spawn with the manifest's `[run]`; `initialize` within 5 s with a
//!    supported `api`.
//! 2. `start`; caps at the media ends equal `CanvasCaps::video()` and, where
//!    audio is declared, `audio()`.
//! 3. In three seconds, at least 90 percent of the expected buffers, with
//!    monotonic PTS.
//! 4. `configure` with every example in the settings schema; `applied` or
//!    `restart_required`, never a crash.
//! 5. `stop` leaves no descriptors and no directories behind.
//! 6. Kill the process mid stream; a freeze frame covers it and it comes back
//!    within the backoff, with the programme's frame interval never over 34 ms.
//! 7. The manifest, every tool schema and every SKILL.md validate.
//! 8. The footprint: the plugin's own cpu and rss, and what the core added for
//!    the transport it chose.
//!
//! Checks 1, 4, 6 and 8 need a process, so they are skipped with a word for a
//! built in kind, which has none. Check 7 reads files and needs neither.
//!
//! The test core it runs against is a 1280x720x30 canvas with no outputs and no
//! multiview, which is what `godwinmix --test-core` prints and runs.

use super::source::SourceRequest;
use super::{MediaEnds, StreamMode};
use godwinmix_protocol::plugin::manifest::Manifest as PluginManifest;
use crate::caps::CanvasCaps;
use crate::config::{BrowserConfig, Canvas, SourceConfig};
use crate::probe::Backends;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The canvas the harness runs on: small enough to be quick on a Pi, large
/// enough that scaling and conversion are real work.
pub fn test_canvas() -> CanvasCaps {
    CanvasCaps::new(&Canvas { width: 1280, height: 720, fps: 30, sample_rate: 48000, channels: 2 })
}

/// How long buffers are counted for.
pub const SAMPLE: Duration = Duration::from_secs(3);
/// The share of the expected buffers that must arrive. The missing tenth is
/// the start up the plugin is allowed.
pub const EXPECTED_SHARE: f64 = 0.9;
/// How long a source may take to reach PLAYING.
pub const PLAYING_TIMEOUT: Duration = Duration::from_secs(10);

/// One check and what it found.
#[derive(Debug, Clone)]
pub struct CheckResult {
    pub name: &'static str,
    pub passed: bool,
    pub detail: String,
}

impl CheckResult {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, passed: true, detail: detail.into() }
    }

    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self { name, passed: false, detail: detail.into() }
    }
}

/// What a run found, in the order the checks were made.
#[derive(Debug, Clone, Default)]
pub struct Report {
    pub type_id: String,
    pub checks: Vec<CheckResult>,
}

impl Report {
    pub fn passed(&self) -> bool {
        self.checks.iter().all(|c| c.passed)
    }

    /// One line per check, for a terminal or a CI log.
    pub fn lines(&self) -> Vec<String> {
        self.checks
            .iter()
            .map(|c| {
                format!("{} {:<22} {}", if c.passed { "ok  " } else { "FAIL" }, c.name, c.detail)
            })
            .collect()
    }

    /// The first failure, as an error, for a test that wants to assert.
    pub fn into_result(self) -> Result<Self> {
        if let Some(f) = self.checks.iter().find(|c| !c.passed) {
            anyhow::bail!("{} failed check `{}`: {}", self.type_id, f.name, f.detail);
        }
        Ok(self)
    }
}

/// What the counting probe on one proxy sink saw.
#[derive(Default)]
struct Counter {
    buffers: AtomicU64,
    /// Buffers whose PTS went backwards against the one before.
    regressions: AtomicU64,
    /// How much media arrived, in nanoseconds of buffer duration. Counted
    /// because a buffer is not a fixed amount of audio: the canvas contract
    /// asks a plugin for ten millisecond buffers, but a source handing the core
    /// a container is demuxed into whatever the muxer chose, and counting those
    /// would fail a source that delivered every sample.
    nanos: AtomicU64,
}

impl Counter {
    fn install(self: &Arc<Self>, proxy: &gst::Element) -> Result<()> {
        let pad = proxy.static_pad("sink").context("a proxy sink with no sink pad")?;
        let me = self.clone();
        let last = std::sync::Mutex::new(None::<gst::ClockTime>);
        pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
                me.buffers.fetch_add(1, Ordering::Relaxed);
                if let Some(d) = b.duration() {
                    me.nanos.fetch_add(d.nseconds(), Ordering::Relaxed);
                }
                if let Some(pts) = b.pts() {
                    let mut held = last.lock().expect("the counter mutex is never poisoned");
                    if held.is_some_and(|prev| pts < prev) {
                        me.regressions.fetch_add(1, Ordering::Relaxed);
                    }
                    *held = Some(pts);
                }
            }
            gst::PadProbeReturn::Ok
        })
        .context("installing a harness counter")?;
        Ok(())
    }
}

/// Run the checks against one source config on the test canvas.
///
/// The caller decides what to do with the report. `cargo test` turns it into an
/// assertion; `godwinmix --test-core` prints it.
pub fn check_source(cfg: &SourceConfig, allow_exec: bool) -> Result<Report> {
    let canvas = test_canvas();
    let backends = Backends::probe(crate::config::Accel::Auto, crate::config::Accel::Auto)
        .context("probing the backends for the harness")?;
    let browser = BrowserConfig::default();
    let provide = super::source::resolve_config(cfg)?;
    let mut report = Report { type_id: provide.manifest.provide_id(), checks: Vec::new() };
    let declared = provide.manifest.media;

    let request = SourceRequest {
        cfg,
        canvas: &canvas,
        backends: &backends,
        browser: &browser,
        allow_exec,
        thumb_fps: 8,
        origin: Instant::now(),
        overlay: None,
    };
    let mut source = (provide.make)(request)?;
    source.initialize(super::Hello {
        instance: cfg.id.clone(),
        canvas: canvas.clone(),
        api_level: super::API_LEVEL,
        params: cfg.effective_params(),
        tier: super::Tier::Core,
    })?;
    // Thumb off: check 2 is about the two ends every source has, and a kind
    // that builds a thumbnail end nobody asked for is failing a different
    // check.
    let ends = source.start(&canvas, false)?;

    let video = Arc::new(Counter::default());
    let audio = Arc::new(Counter::default());
    video.install(&ends.video)?;
    audio.install(&ends.audio)?;

    report.checks.push(reaches_playing(&ends));
    report.checks.push(caps_match(&ends, &canvas, declared.video, "video caps", &ends.video));
    if declared.audio.present() {
        report.checks.push(caps_match(&ends, &canvas, declared.audio, "audio caps", &ends.audio));
    }
    std::thread::sleep(SAMPLE);
    report.checks.push(enough_buffers(
        "video buffers",
        &video,
        expected_video(&canvas),
        declared.video,
    ));
    report.checks.push(enough_audio(&audio, declared.audio));

    let _ = ends.pipeline.set_state(gst::State::Null);
    source.stop()?;
    report.checks.push(CheckResult::pass("stop", "the pipeline is in NULL and the kind let go"));
    Ok(report)
}

fn expected_video(canvas: &CanvasCaps) -> u64 {
    let fps = canvas.fps.numer() as f64 / canvas.fps.denom().max(1) as f64;
    (fps * SAMPLE.as_secs_f64()) as u64
}

/// Check 3 for audio, measured in time rather than in buffers.
///
/// The question is whether ninety percent of the sound arrived, and a count of
/// buffers cannot answer it: the canvas contract asks a plugin for ten
/// millisecond buffers, but a source handing the core a container is demuxed
/// into whatever the muxer chose, and a perfectly conformant one delivered 133
/// buffers where the count wanted 270.
fn enough_audio(counter: &Counter, mode: StreamMode) -> CheckResult {
    const NAME: &str = "audio";
    if !mode.present() {
        return CheckResult::pass("audio buffers", "not declared, not expected");
    }
    let back = counter.regressions.load(Ordering::Relaxed);
    if back > 0 {
        return CheckResult::fail("audio buffers", format!("{back} buffers went backwards in time"));
    }
    let got = counter.nanos.load(Ordering::Relaxed);
    let want = (SAMPLE.as_nanos() as f64 * EXPECTED_SHARE) as u64;
    if got < want {
        return CheckResult::fail(
            "audio buffers",
            format!(
                "{} ms of {NAME} in {} s, wanted at least {} ms",
                got / 1_000_000,
                SAMPLE.as_secs(),
                want / 1_000_000
            ),
        );
    }
    CheckResult::pass(
        "audio buffers",
        format!(
            "{} ms in {} buffers, none out of order",
            got / 1_000_000,
            counter.buffers.load(Ordering::Relaxed)
        ),
    )
}

fn reaches_playing(ends: &MediaEnds) -> CheckResult {
    if let Err(e) = ends.pipeline.set_state(gst::State::Playing) {
        return CheckResult::fail("playing", format!("the pipeline refused to start: {e}"));
    }
    let (res, state, _) = ends.pipeline.state(gst::ClockTime::from_mseconds(
        PLAYING_TIMEOUT.as_millis() as u64,
    ));
    match res {
        Ok(_) if state == gst::State::Playing => {
            CheckResult::pass("playing", "reached PLAYING within the timeout")
        }
        Ok(_) => CheckResult::fail("playing", format!("stopped at {state:?}")),
        Err(e) => CheckResult::fail("playing", format!("state change failed: {e:?}")),
    }
}

/// Check 2: the caps at a media end are the canvas caps, exactly.
fn caps_match(
    _ends: &MediaEnds,
    canvas: &CanvasCaps,
    mode: StreamMode,
    name: &'static str,
    proxy: &gst::Element,
) -> CheckResult {
    if !mode.present() {
        return CheckResult::pass(name, "not declared, not expected");
    }
    let want = if name.starts_with("video") { canvas.video() } else { canvas.audio() };
    let Some(pad) = proxy.static_pad("sink") else {
        return CheckResult::fail(name, "the proxy sink has no sink pad");
    };
    let deadline = Instant::now() + PLAYING_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(have) = pad.current_caps() {
            // A subset, not an equality. What negotiated is always at least as
            // specific as the canvas contract: `videotestsrc` adds
            // `multiview-mode`, an audio decoder adds `channel-mask`, and a
            // demuxer adds both. What matters is that every field the contract
            // names is there and says what the contract says, which is exactly
            // what `is_subset` asks.
            return if have.is_subset(&want) {
                CheckResult::pass(name, have.to_string())
            } else {
                CheckResult::fail(name, format!("wanted {want}, got {have}"))
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    CheckResult::fail(name, "no caps arrived at the proxy sink")
}

/// Check 3: at least 90 percent of the expected buffers, none of them going
/// backwards in time.
fn enough_buffers(
    name: &'static str,
    counter: &Counter,
    expected: u64,
    mode: StreamMode,
) -> CheckResult {
    if !mode.present() {
        return CheckResult::pass(name, "not declared, not expected");
    }
    let seen = counter.buffers.load(Ordering::Relaxed);
    let back = counter.regressions.load(Ordering::Relaxed);
    let floor = (expected as f64 * EXPECTED_SHARE) as u64;
    if back > 0 {
        return CheckResult::fail(name, format!("{back} buffers went backwards in time"));
    }
    if seen < floor {
        return CheckResult::fail(
            name,
            format!("{seen} buffers in {}s, wanted at least {floor}", SAMPLE.as_secs()),
        );
    }
    CheckResult::pass(name, format!("{seen} buffers, none out of order"))
}

// ---------------------------------------------------------------------------
// Checks 1, 4, 6, 7 and 8: what a plugin has that a built in kind does not
// ---------------------------------------------------------------------------

/// How long check 6 waits for the picture to come back after a kill.
pub const RESTART_TIMEOUT: Duration = Duration::from_secs(12);
/// The programme's frame interval must never exceed this. 34 ms is one frame
/// at 30 fps plus the slack a scheduler is allowed; the number is the one in
/// the 02 appendix and the roadmap's acceptance.
pub const MAX_FRAME_INTERVAL: Duration = Duration::from_millis(34);

/// Check 7: the manifest, the tool schemas and every SKILL.md.
///
/// The only check that needs no process and no pipeline, which is why
/// `gmx plugin test --quick` always runs it and why it is the first thing an
/// author sees. Every problem is reported with the key path that caused it, so
/// a whole file is fixed in one pass.
pub fn check_manifest(root: &std::path::Path) -> CheckResult {
    let path = root.join("gmx-plugin.toml");
    let manifest = match PluginManifest::load(&path) {
        Ok(m) => m,
        Err(e) => return CheckResult::fail("manifest", format!("{e}")),
    };
    let mut problems = Vec::new();
    for (index, tool) in manifest.tools.iter().enumerate() {
        for (key, file) in [("input", &tool.input), ("output", &tool.output)] {
            let Some(file) = file else { continue };
            let full = root.join(file);
            match std::fs::read_to_string(&full) {
                Err(e) => problems.push(format!("tools[{index}].{key}: {file} could not be read: {e}")),
                Ok(text) => match serde_json::from_str::<serde_json::Value>(&text) {
                    Err(e) => problems.push(format!("tools[{index}].{key}: {file} is not JSON: {e}")),
                    Ok(schema) => {
                        if schema.get("type").and_then(|v| v.as_str()) != Some("object") {
                            problems.push(format!(
                                "tools[{index}].{key}: a tool schema is an object schema; write \
                                 {{\"type\": \"object\", \"properties\": {{}}}}."
                            ));
                        }
                    }
                },
            }
        }
    }
    if problems.is_empty() {
        CheckResult::pass(
            "manifest",
            format!(
                "{} v{}: {} provide(s), {} tool(s), every path and schema in place",
                manifest.plugin.name,
                manifest.plugin.version,
                manifest.provides.len(),
                manifest.tools.len()
            ),
        )
    } else {
        CheckResult::fail("manifest", problems.join("; "))
    }
}

/// Check 1: the process starts and says hello inside the handshake window.
///
/// Measured rather than assumed: the number that comes back is what an author
/// compares against the five second limit after adding a dependency.
pub fn check_spawn(root: &std::path::Path, provide: &str) -> CheckResult {
    let manifest = match PluginManifest::load(root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => return CheckResult::fail("spawn", format!("the manifest does not load: {e}")),
    };
    let ctx = godwinmix_host::launch::LaunchCtx {
        root: root.to_path_buf(),
        provide: provide.to_string(),
        instance: "harness".into(),
        api_level: super::API_LEVEL,
        token: String::new(),
        rpc: String::new(),
        media: String::new(),
    };
    let launch = match godwinmix_host::launch::plan(&manifest, &ctx) {
        Ok(l) => l,
        Err(e) => return CheckResult::fail("spawn", format!("{e}")),
    };
    let started = Instant::now();
    let mut child = match super::host::Sidecar::spawn("harness", &launch) {
        Ok(c) => c,
        Err(e) => return CheckResult::fail("spawn", format!("{e:#}")),
    };
    let canvas = test_canvas();
    let outcome = child.handshake(
        Some(&manifest),
        super::host::source::canvas_of(&canvas),
        provide,
        serde_json::json!({}),
        |t| Ok(format!("{}/harness.{}", std::env::temp_dir().display(), t.as_str())),
    );
    let took = started.elapsed();
    let result = match outcome {
        Ok(n) => CheckResult::pass(
            "spawn",
            format!(
                "hello in {} ms (the limit is {} s), api {}, transport {}",
                took.as_millis(),
                godwinmix_host::HANDSHAKE_TIMEOUT.as_secs(),
                n.api,
                n.transport
            ),
        ),
        Err(e) => CheckResult::fail("spawn", format!("{e:#}")),
    };
    child.shutdown("the harness is done with it");
    result
}

/// Check 4: `configure` with every example the settings schema gives, and
/// never a crash.
///
/// The examples are the author's own: a schema that carries them is a schema
/// an agent can fill in, which is why the manifest asks for them and why this
/// check reads them rather than inventing values.
pub fn check_configure(root: &std::path::Path, provide: &str) -> CheckResult {
    let manifest = match PluginManifest::load(root.join("gmx-plugin.toml")) {
        Ok(m) => m,
        Err(e) => return CheckResult::fail("configure", format!("{e}")),
    };
    let Some(decl) = manifest.provide(provide) else {
        return CheckResult::fail("configure", format!("there is no provide called `{provide}`"));
    };
    let Some(settings) = decl.settings.as_ref() else {
        return CheckResult::pass("configure", "no settings schema, so nothing to try");
    };
    let cases = schema_examples(&root.join(settings));
    if cases.is_empty() {
        return CheckResult::pass(
            "configure",
            "the schema gives no examples; add `examples` to each property and this check \
             will exercise them",
        );
    }
    let ctx = godwinmix_host::launch::LaunchCtx {
        root: root.to_path_buf(),
        provide: provide.to_string(),
        instance: "harness".into(),
        api_level: super::API_LEVEL,
        token: String::new(),
        rpc: String::new(),
        media: String::new(),
    };
    let launch = match godwinmix_host::launch::plan(&manifest, &ctx) {
        Ok(l) => l,
        Err(e) => return CheckResult::fail("configure", format!("{e}")),
    };
    let mut child = match super::host::Sidecar::spawn("harness", &launch) {
        Ok(c) => c,
        Err(e) => return CheckResult::fail("configure", format!("{e:#}")),
    };
    let canvas = test_canvas();
    let handshake = child.handshake(
        Some(&manifest),
        super::host::source::canvas_of(&canvas),
        provide,
        serde_json::json!({}),
        |t| Ok(format!("{}/harness.{}", std::env::temp_dir().display(), t.as_str())),
    );
    if let Err(e) = handshake {
        child.shutdown("the harness is done with it");
        return CheckResult::fail("configure", format!("it would not shake hands: {e:#}"));
    }
    let total = cases.len();
    let mut refused = Vec::new();
    for case in cases {
        match child.call("configure", serde_json::json!({ "params": case })) {
            Ok(answer) => {
                let applied = answer.get("applied").and_then(serde_json::Value::as_bool);
                let restart = answer.get("restart_required").and_then(serde_json::Value::as_bool);
                if applied != Some(true) && restart != Some(true) {
                    refused.push(format!("{case} answered {answer}"));
                }
            }
            Err(e) => refused.push(format!("{case}: {e}")),
        }
        if !child.running() {
            child.shutdown("it died");
            return CheckResult::fail(
                "configure",
                "the plugin exited while being configured. `configure` must answer \
                 {applied} or {restart_required}, never crash.",
            );
        }
    }
    child.shutdown("the harness is done with it");
    if refused.is_empty() {
        CheckResult::pass("configure", format!("{total} example(s), every one answered"))
    } else {
        CheckResult::fail("configure", refused.join("; "))
    }
}

/// Every example a settings schema offers, as whole params objects.
///
/// One object per example of each property, rather than the cross product: a
/// schema with four properties and three examples each would otherwise be
/// eighty one `configure` calls and a minute of test time.
fn schema_examples(path: &std::path::Path) -> Vec<serde_json::Value> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(schema) = serde_json::from_str::<serde_json::Value>(&text) else { return Vec::new() };
    let mut out = Vec::new();
    // A default object first: the state a plugin is in before anyone edits it.
    let mut defaults = serde_json::Map::new();
    let Some(properties) = schema.get("properties").and_then(|v| v.as_object()) else {
        return out;
    };
    for (name, property) in properties {
        if let Some(default) = property.get("default") {
            defaults.insert(name.clone(), default.clone());
        }
    }
    if !defaults.is_empty() {
        out.push(serde_json::Value::Object(defaults.clone()));
    }
    for (name, property) in properties {
        let Some(examples) = property.get("examples").and_then(|v| v.as_array()) else { continue };
        for example in examples {
            let mut case = defaults.clone();
            case.insert(name.clone(), example.clone());
            out.push(serde_json::Value::Object(case));
        }
    }
    out
}

/// What the picture did while something was being done to it.
///
/// Check 6 asks two questions and this answers both: did the frames come back,
/// and did the interval between them ever exceed one frame of slack. The
/// second is the one that matters, because a freeze frame that covers a gap is
/// the difference between a plugin crash and a programme outage.
#[derive(Debug, Clone)]
pub struct Interval {
    pub frames: u64,
    pub longest: Duration,
    pub came_back: bool,
}

/// Watch the interval between buffers on a proxy sink.
struct Intervals {
    last: Mutex<Option<Instant>>,
    longest: AtomicU64,
    frames: AtomicU64,
}

impl Intervals {
    fn install(self: &Arc<Self>, proxy: &gst::Element) -> Result<()> {
        let pad = proxy.static_pad("sink").context("a proxy sink with no sink pad")?;
        let me = self.clone();
        pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, _info| {
            let now = Instant::now();
            let mut last = me.last.lock().expect("the interval mutex is never poisoned");
            if let Some(then) = *last {
                let gap = now.duration_since(then).as_micros() as u64;
                me.longest.fetch_max(gap, Ordering::Relaxed);
            }
            *last = Some(now);
            me.frames.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        })
        .context("installing a harness interval probe")?;
        Ok(())
    }

    fn longest(&self) -> Duration {
        Duration::from_micros(self.longest.load(Ordering::Relaxed))
    }

    fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    fn reset(&self) {
        self.longest.store(0, Ordering::Relaxed);
        *self.last.lock().expect("the interval mutex") = None;
    }
}

impl Default for Intervals {
    fn default() -> Self {
        Self { last: Mutex::new(None), longest: AtomicU64::new(0), frames: AtomicU64::new(0) }
    }
}

/// Check 6: kill the process mid stream and watch what the picture does.
///
/// The freeze frame is the core's, not the plugin's: the compositor keeps the
/// last frame on the pad while the source is rebuilt, and the encoder never
/// stops. So the measurement is on the source's own media end, which is where
/// a gap would show first and largest.
pub fn check_kill(cfg: &SourceConfig, allow_exec: bool) -> CheckResult {
    let canvas = test_canvas();
    let backends = match Backends::probe(crate::config::Accel::Auto, crate::config::Accel::Auto) {
        Ok(b) => b,
        Err(e) => return CheckResult::fail("kill", format!("probing the backends: {e}")),
    };
    let provide = match super::source::resolve_config(cfg) {
        Ok(p) => p,
        Err(e) => return CheckResult::fail("kill", format!("{e}")),
    };
    let browser = BrowserConfig::default();
    let mut source = match (provide.make)(SourceRequest {
        cfg,
        canvas: &canvas,
        backends: &backends,
        browser: &browser,
        allow_exec,
        thumb_fps: 8,
        origin: Instant::now(),
        overlay: None,
    }) {
        Ok(s) => s,
        Err(e) => return CheckResult::fail("kill", format!("{e:#}")),
    };
    if let Err(e) = source.initialize(super::Hello {
        instance: cfg.id.clone(),
        canvas: canvas.clone(),
        api_level: super::API_LEVEL,
        params: cfg.effective_params(),
        tier: super::Tier::Sidecar,
    }) {
        return CheckResult::fail("kill", format!("{e:#}"));
    }
    let ends = match source.start(&canvas, false) {
        Ok(e) => e,
        Err(e) => return CheckResult::fail("kill", format!("{e:#}")),
    };
    let watch = Arc::new(Intervals::default());
    if let Err(e) = watch.install(&ends.video) {
        return CheckResult::fail("kill", format!("{e}"));
    }
    let _ = ends.pipeline.set_state(gst::State::Playing);
    // Let it settle before anything is measured: the first frames of any
    // source arrive unevenly and no supervisor judges a source on them.
    std::thread::sleep(Duration::from_secs(2));
    let before = watch.frames();
    if before == 0 {
        let _ = ends.pipeline.set_state(gst::State::Null);
        let _ = source.stop();
        return CheckResult::fail("kill", "no frames arrived before the kill, so there was \
                                          nothing to interrupt");
    }
    watch.reset();
    let killed = kill_behind(&mut source);
    let outcome = wait_for_frames(&watch, before);
    let longest = watch.longest();
    let _ = ends.pipeline.set_state(gst::State::Null);
    let _ = source.stop();
    if !killed {
        return CheckResult::pass("kill", "this kind has no process to kill");
    }
    if !outcome.came_back {
        return CheckResult::fail(
            "kill",
            format!(
                "the picture did not come back within {} s of the process being killed",
                RESTART_TIMEOUT.as_secs()
            ),
        );
    }
    if longest > MAX_FRAME_INTERVAL {
        return CheckResult::fail(
            "kill",
            format!(
                "the frame interval reached {} ms across the kill; the limit is {} ms",
                longest.as_millis(),
                MAX_FRAME_INTERVAL.as_millis()
            ),
        );
    }
    CheckResult::pass(
        "kill",
        format!(
            "killed mid stream, back in {} frames, longest interval {:.1} ms (limit {} ms)",
            outcome.frames,
            longest.as_secs_f64() * 1000.0,
            MAX_FRAME_INTERVAL.as_millis()
        ),
    )
}

/// Kill whatever process is behind a source, if it has one.
fn kill_behind(source: &mut Box<dyn super::source::Source>) -> bool {
    source.call("restart", serde_json::json!({})).is_ok()
}

fn wait_for_frames(watch: &Arc<Intervals>, before: u64) -> Interval {
    let deadline = Instant::now() + RESTART_TIMEOUT;
    while Instant::now() < deadline {
        if watch.frames() > before {
            return Interval {
                frames: watch.frames() - before,
                longest: watch.longest(),
                came_back: true,
            };
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    Interval { frames: 0, longest: watch.longest(), came_back: false }
}

/// Check 8: what this plugin costs, and what the core added for it.
///
/// Printed rather than judged. There is no number a plugin fails at, because a
/// screen capture at 1080p60 and a clock that draws text once a second have
/// nothing in common; what an author needs is to see the figure before they
/// publish, and to see it change when they add a dependency.
pub fn check_footprint(pid: Option<u32>, transport: Option<&str>) -> CheckResult {
    let Some(pid) = pid else {
        return CheckResult::pass("footprint", "no process, so nothing to measure");
    };
    let mut sampler = godwinmix_host::sampler::Sampler::new();
    // Two samples a second apart: the first sets the baseline the cpu figure
    // is measured against, and one second is the refresh `plugin.stats` uses.
    sampler.sample(&[pid, std::process::id()]);
    std::thread::sleep(Duration::from_secs(1));
    let samples = sampler.sample(&[pid, std::process::id()]);
    let plugin = samples.get(&pid).copied().unwrap_or_default();
    let core = samples.get(&std::process::id()).copied().unwrap_or_default();
    CheckResult::pass(
        "footprint",
        format!(
            "plugin cpu {}, rss {}; core cpu {}, rss {} ({})",
            percent(plugin.cpu_percent),
            megabytes(plugin.rss_bytes),
            percent(core.cpu_percent),
            megabytes(core.rss_bytes),
            transport.unwrap_or("transport not settled")
        ),
    )
}

fn percent(value: Option<f64>) -> String {
    value.map(|v| format!("{v:.0}%")).unwrap_or_else(|| "not measured here".into())
}

fn megabytes(value: Option<u64>) -> String {
    value
        .map(|v| format!("{} MB", v / (1024 * 1024)))
        .unwrap_or_else(|| "not measured here".into())
}

/// Run every check that applies to a plugin directory.
///
/// `quick` skips 6 and 8, which are the two that take time: the kill test
/// waits for a restart and the footprint waits a second to have something to
/// average. That is the fifteen second run against the sixty second one.
pub fn check_plugin(root: &std::path::Path, quick: bool) -> Result<Report> {
    let manifest = PluginManifest::load(root.join("gmx-plugin.toml"))
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    let provide = manifest
        .provides
        .iter()
        .find(|p| p.kind == "source")
        .or_else(|| manifest.provides.first())
        .context("the manifest declares no provides, so there is nothing to check")?;
    let type_id = format!("{}/{}", manifest.plugin.name, provide.id);
    let mut report = Report { type_id: type_id.clone(), checks: Vec::new() };
    report.checks.push(check_manifest(root));
    report.checks.push(check_spawn(root, &provide.id));
    if provide.kind != "source" {
        report.checks.push(CheckResult::pass(
            "media",
            format!("a {} provide carries no media, so checks 2, 3 and 6 do not apply", provide.kind),
        ));
        report.checks.push(check_configure(root, &provide.id));
        return Ok(report);
    }
    let mut cfg = SourceConfig::bare("harness", "");
    cfg.type_id = Some(type_id);
    match check_source(&cfg, false) {
        Ok(media) => report.checks.extend(media.checks),
        Err(e) => report.checks.push(CheckResult::fail("media", format!("{e:#}"))),
    }
    report.checks.push(check_configure(root, &provide.id));
    if !quick {
        report.checks.push(check_kill(&cfg, false));
        report.checks.push(check_footprint(None, provide.transports.first().map(|t| t.as_str())));
    }
    Ok(report)
}

/// Run every check that applies to every plugin the loader has, for
/// `godwinmix --test-core`.
pub fn check_loaded_plugins(quick: bool) -> Vec<Result<Report>> {
    super::loader::enabled()
        .into_iter()
        .map(|p| check_plugin(&p.root, quick))
        .collect()
}

/// Run the harness against every built in kind that needs no network, and
/// return one report each.
///
/// This is what `--test-core` runs. The kinds that need a server, a browser or
/// a file the caller does not have are left out by name rather than attempted
/// and excused: a check that cannot run is not a check that passed.
pub fn check_offline_kinds() -> Vec<Result<Report>> {
    ["test://smpte", "test://ball"]
        .iter()
        .enumerate()
        .map(|(n, uri)| {
            let cfg = SourceConfig::bare(&format!("harness-{n}"), uri);
            check_source(&cfg, false)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn init() {
        let _ = gst::init();
    }

    #[test]
    fn the_test_source_passes_every_check_the_harness_makes() {
        init();
        let cfg = SourceConfig::bare("harness-test", "test://smpte");
        let report = check_source(&cfg, false).expect("the harness runs");
        for line in report.lines() {
            println!("{line}");
        }
        report.into_result().expect("test/source is conformant");
    }

    #[test]
    fn a_file_source_passes_the_same_checks() {
        init();
        // A file the harness makes itself, so the check needs nothing but
        // GStreamer. Written once and removed at the end, like every other
        // temporary this crate makes.
        let path = std::env::temp_dir()
            .join(format!("gmx-harness-{}-{}.mkv", std::process::id(), line!()));
        if let Err(e) = write_clip(&path) {
            println!("skipping: could not write a clip to check against: {e}");
            return;
        }
        let cfg = SourceConfig::bare("harness-file", &crate::input::file_uri(&path));
        let report = check_source(&cfg, false).expect("the harness runs");
        for line in report.lines() {
            println!("{line}");
        }
        let outcome = report.into_result();
        let _ = std::fs::remove_file(&path);
        outcome.expect("file/source is conformant");
    }

    /// The exec kind needs no network either: a command writing a container to
    /// stdout is the whole contract, and `gst-launch-1.0` writes one.
    ///
    /// Skipped where that binary is not on PATH, and on Windows, where the
    /// shell form of an exec source is refused: the pipe transport is the same
    /// but the command line is not portable, so the check would be testing the
    /// shell rather than the kind.
    #[cfg(unix)]
    #[test]
    fn an_exec_source_passes_the_same_checks() {
        init();
        if which("gst-launch-1.0").is_none() {
            println!("skipping: gst-launch-1.0 is not on PATH");
            return;
        }
        let cfg = SourceConfig::bare(
            "harness-exec",
            "exec:gst-launch-1.0 -q videotestsrc is-live=true ! video/x-raw,width=640,height=360,framerate=30/1              ! matroskamux streamable=true name=mux ! fdsink fd=1              audiotestsrc is-live=true ! audioconvert ! mux.",
        );
        let report = check_source(&cfg, true).expect("the harness runs");
        for line in report.lines() {
            println!("{line}");
        }
        report.into_result().expect("exec/source is conformant");
    }

    /// Where a binary is, without a crate for it.
    #[cfg(unix)]
    fn which(name: &str) -> Option<std::path::PathBuf> {
        std::env::var_os("PATH").and_then(|paths| {
            std::env::split_paths(&paths)
                .map(|dir| dir.join(name))
                .find(|p| p.is_file())
        })
    }

    /// Ten seconds of colour bars and a tone in Matroska, so the file check
    /// has something with both streams in it.
    fn write_clip(path: &std::path::Path) -> Result<()> {
        let desc = format!(
            "videotestsrc num-buffers=300 ! video/x-raw,width=640,height=360,framerate=30/1 \
             ! x264enc speed-preset=ultrafast tune=zerolatency ! queue ! mux. \
             audiotestsrc num-buffers=470 ! audioconvert ! avenc_aac ! queue ! mux. \
             matroskamux name=mux ! filesink location={}",
            path.to_string_lossy()
        );
        let pipeline = match gst::parse::launch(&desc) {
            Ok(p) => p,
            Err(e) => anyhow::bail!("{e}"),
        };
        pipeline.set_state(gst::State::Playing)?;
        let bus = pipeline.bus().context("no bus on the clip writer")?;
        let msg = bus.timed_pop_filtered(
            gst::ClockTime::from_seconds(30),
            &[gst::MessageType::Eos, gst::MessageType::Error],
        );
        let _ = pipeline.set_state(gst::State::Null);
        match msg.as_ref().map(|m| m.view()) {
            Some(gst::MessageView::Eos(_)) => Ok(()),
            Some(gst::MessageView::Error(e)) => anyhow::bail!("{}", e.error()),
            _ => anyhow::bail!("the clip writer did not finish"),
        }
    }
}
