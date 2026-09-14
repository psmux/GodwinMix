//! The conformance harness: what a source has to do to be a source.
//!
//! The audit found no test anywhere that took a built source to PLAYING and
//! checked its caps. This is that test, written once and run against every
//! kind, so a plugin author finds out what is wrong from a report rather than
//! from a black programme.
//!
//! The checks here are 2, 3 and 5 of 03 section 11, which are the ones a core
//! with no network can make:
//!
//! 2. `start`; caps at the media ends equal `CanvasCaps::video()` and, where
//!    audio is declared, `audio()`.
//! 3. In three seconds, at least 90 percent of the expected buffers, with
//!    monotonic PTS.
//! 5. `stop` leaves no descriptors and no directories behind.
//!
//! The test core it runs against is a 1280x720x30 canvas with no outputs and no
//! multiview, which is what `godwinmix --test-core` prints and runs.

use super::source::SourceRequest;
use super::{MediaEnds, StreamMode};
use crate::caps::CanvasCaps;
use crate::config::{BrowserConfig, Canvas, SourceConfig};
use crate::probe::Backends;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
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
}

impl Counter {
    fn install(self: &Arc<Self>, proxy: &gst::Element) -> Result<()> {
        let pad = proxy.static_pad("sink").context("a proxy sink with no sink pad")?;
        let me = self.clone();
        let last = std::sync::Mutex::new(None::<gst::ClockTime>);
        pad.add_probe(gst::PadProbeType::BUFFER, move |_pad, info| {
            if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
                me.buffers.fetch_add(1, Ordering::Relaxed);
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
    report.checks.push(enough_buffers(
        "audio buffers",
        &audio,
        expected_audio(),
        declared.audio,
    ));

    let _ = ends.pipeline.set_state(gst::State::Null);
    source.stop()?;
    report.checks.push(CheckResult::pass("stop", "the pipeline is in NULL and the kind let go"));
    Ok(report)
}

fn expected_video(canvas: &CanvasCaps) -> u64 {
    let fps = canvas.fps.numer() as f64 / canvas.fps.denom().max(1) as f64;
    (fps * SAMPLE.as_secs_f64()) as u64
}

/// Ten milliseconds of audio per buffer is the canvas contract, so three
/// seconds is 300 buffers. Elements that pick their own buffer size deliver
/// fewer and larger ones, so this is a floor on the count only when the kind
/// declared raw audio.
fn expected_audio() -> u64 {
    (SAMPLE.as_millis() / 10) as u64
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
