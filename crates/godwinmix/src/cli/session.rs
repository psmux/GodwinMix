//! `gmx session show`, `replay` and `diff`: the session log as an artefact.
//!
//! The session log is already every command, every event and every supervisor
//! decision in order (10 section 2). This turns it from a file somebody greps
//! into the thing 10 section 5 asks for: a bug from a church hall on a Sunday
//! becomes a file that fails on a laptop on Monday.
//!
//! ```text
//! gmx session show   run.jsonl --from 20:13 --to 20:15   a timeline a person reads
//! gmx session replay run.jsonl --against test-core        the same commands, same timing
//! gmx session diff   before.jsonl after.jsonl             what changed between two runs
//! ```
//!
//! Replay re-issues the recorded commands against a core built in this
//! process, with every source replaced by a deterministic `test/source` (or by
//! `--source-fixture`), and compares the state deltas it produces against the
//! ones in the file. Timestamps, sequence numbers, running times and trace ids
//! are expected to differ and are not compared; what is on air, which sources
//! are live and what the outputs are doing are not expected to differ and are.

use anyhow::{Context, Result};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub mod core;
pub mod delta;
pub mod show;

pub use delta::{Delta, Expectations};

#[derive(Debug, Clone, clap::Args)]
pub struct SessionArgs {
    #[command(subcommand)]
    pub cmd: SessionCmd,
}

#[derive(Debug, Clone, clap::Subcommand)]
pub enum SessionCmd {
    /// Print a session log as a timeline a person can read.
    Show {
        /// The JSONL file. `gmx --info` says where the running core's is.
        file: PathBuf,
        /// Only records at or after this timestamp. A prefix is enough:
        /// `--from 20:13` matches the first record whose time starts there.
        #[arg(long)]
        from: Option<String>,
        /// Only records at or before this timestamp.
        #[arg(long)]
        to: Option<String>,
        /// Every record, including the ones the timeline folds away.
        #[arg(long)]
        all: bool,
    },
    /// Re-issue a session's commands against a test core and compare.
    ///
    /// Exits non zero when a state delta differs, which is what makes a
    /// recorded session a regression test.
    Replay {
        /// The JSONL file to replay.
        file: PathBuf,
        /// What to replay against. Only `test-core` exists: a core built in
        /// this process, 1280x720x30, no outputs, no multiview.
        #[arg(long, default_value = "test-core")]
        against: String,
        /// A media file to stand in for every source, instead of colour bars.
        /// The file a support bundle carries for the camera that misbehaved.
        #[arg(long)]
        source_fixture: Option<PathBuf>,
        /// Compare against this file rather than against the log's own
        /// events. Written by hand when a session is turned into a test.
        #[arg(long)]
        expect: Option<PathBuf>,
        /// Replay the commands as fast as they will go instead of at the
        /// timing they were recorded at.
        #[arg(long)]
        fast: bool,
        /// Print the deltas the replay produced, as `expect_changes.json`.
        /// This is how a new case in `tests/sessions/` gets its expectations.
        #[arg(long)]
        write_expectations: bool,
        /// Install a plugin into the test core before replaying, by path.
        /// Repeat it for several. This is how a session is replayed with and
        /// without a plugin: run it twice and read the difference.
        #[arg(long = "with-plugin", value_name = "DIR")]
        with_plugins: Vec<PathBuf>,
    },
    /// What changed between two session logs.
    Diff {
        a: PathBuf,
        b: PathBuf,
    },
}

pub async fn run(cmd: SessionCmd) -> Result<()> {
    match cmd {
        SessionCmd::Show { file, from, to, all } => {
            let records = read(&file)?;
            print!("{}", show::timeline(&records, from.as_deref(), to.as_deref(), all));
            Ok(())
        }
        SessionCmd::Diff { a, b } => {
            let left = delta::of(&read(&a)?);
            let right = delta::of(&read(&b)?);
            let lines = delta::diff(&left, &right);
            if lines.is_empty() {
                println!(
                    "{} and {} produced the same {} state delta(s)",
                    a.display(),
                    b.display(),
                    left.len()
                );
                return Ok(());
            }
            println!("{}", lines.join("\n"));
            anyhow::bail!("{} state delta(s) differ", lines.len())
        }
        SessionCmd::Replay {
            file,
            against,
            source_fixture,
            expect,
            fast,
            write_expectations,
            with_plugins,
        } => {
            anyhow::ensure!(
                against == "test-core",
                "`--against {against}` is not a thing to replay against. The only one is \
                 `test-core`: a core built in this process with no outputs and no multiview. \
                 Write `--against test-core`."
            );
            let options = Options {
                source_fixture,
                fast,
                expect: expect.clone(),
                write_expectations,
                with_plugins,
            };
            let outcome = replay(&file, &options).await?;
            if write_expectations {
                // The report goes to stderr so stdout is an
                // `expect_changes.json` and nothing else. That is what makes
                // `> tests/sessions/takes.expect_changes.json` work.
                eprint!("{}", outcome.report());
                println!(
                    "{}",
                    outcome.expectations_json().context("those deltas would not serialise")?
                );
                return Ok(());
            }
            print!("{}", outcome.report());
            if !outcome.passed() {
                anyhow::bail!(
                    "{} state delta(s) differ from {}",
                    outcome.differences.len(),
                    expect
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "the recorded session".into())
                );
            }
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Reading the file
// ---------------------------------------------------------------------------

/// One line of a session log, parsed no further than it needs to be.
#[derive(Debug, Clone)]
pub struct Record {
    pub seq: u64,
    pub ts: String,
    pub kind: String,
    pub value: Value,
}

impl Record {
    pub fn method(&self) -> Option<&str> {
        self.value.get("method").and_then(Value::as_str)
    }

    pub fn params(&self) -> Value {
        self.value.get("params").cloned().unwrap_or_else(|| json!({}))
    }

    pub fn event(&self) -> Option<&Value> {
        self.value.get("event")
    }

    /// The event's `type` tag, for a record of kind `event`.
    pub fn event_type(&self) -> Option<&str> {
        self.event()?.get("type").and_then(Value::as_str)
    }
}

/// Read a session log. A line that will not parse is skipped rather than
/// fatal: a log truncated by a power cut is still worth replaying.
pub fn read(path: &Path) -> Result<Vec<Record>> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("there is no session log at {}", path.display()))?;
    Ok(parse(&text))
}

pub fn parse(text: &str) -> Vec<Record> {
    text.lines()
        .filter_map(|line| {
            let value: Value = serde_json::from_str(line).ok()?;
            Some(Record {
                seq: value.get("seq").and_then(Value::as_u64).unwrap_or(0),
                ts: value.get("ts").and_then(Value::as_str).unwrap_or("").to_string(),
                kind: value.get("kind").and_then(Value::as_str).unwrap_or("").to_string(),
                value,
            })
        })
        .collect()
}

/// Milliseconds between two RFC 3339 timestamps as this log writes them.
///
/// Only the time of day is parsed, because a session log is one run and a run
/// that crosses midnight is handled by the clamp below rather than by a date
/// library this crate does not have.
pub fn gap_ms(from: &str, to: &str) -> u64 {
    let ms = |s: &str| -> Option<i64> {
        let time = s.split('T').nth(1)?.trim_end_matches('Z');
        let mut parts = time.split(':');
        let h: i64 = parts.next()?.parse().ok()?;
        let m: i64 = parts.next()?.parse().ok()?;
        let rest = parts.next()?;
        let (sec, frac) = rest.split_once('.').unwrap_or((rest, "0"));
        let s: i64 = sec.parse().ok()?;
        let frac: i64 = format!("{frac:0<3}")[..3].parse().ok()?;
        Some(((h * 60 + m) * 60 + s) * 1000 + frac)
    };
    match (ms(from), ms(to)) {
        (Some(a), Some(b)) if b >= a => (b - a) as u64,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Replay
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Clone)]
pub struct Options {
    pub source_fixture: Option<PathBuf>,
    pub fast: bool,
    pub expect: Option<PathBuf>,
    pub write_expectations: bool,
    /// Plugin directories installed into the test core before the replay.
    pub with_plugins: Vec<PathBuf>,
}

/// What a replay found.
pub struct Outcome {
    pub file: PathBuf,
    /// Commands re-issued.
    pub issued: usize,
    /// Commands the test core cannot take, and why.
    pub skipped: Vec<(String, String)>,
    /// Commands that answered with an error the recording did not have.
    pub errors: Vec<String>,
    pub expected: Vec<Delta>,
    pub produced: Vec<Delta>,
    pub differences: Vec<String>,
    pub took: Duration,
}

impl Outcome {
    pub fn passed(&self) -> bool {
        self.differences.is_empty()
    }

    pub fn report(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::new();
        let _ = writeln!(
            out,
            "replay {} against test-core: {} command(s) in {:.1} s",
            self.file.display(),
            self.issued,
            self.took.as_secs_f64()
        );
        for (method, why) in &self.skipped {
            let _ = writeln!(out, "  skipped {method}: {why}");
        }
        for e in &self.errors {
            let _ = writeln!(out, "  {e}");
        }
        let _ = writeln!(
            out,
            "  {} state delta(s) expected, {} produced",
            self.expected.len(),
            self.produced.len()
        );
        if self.differences.is_empty() {
            let _ = writeln!(out, "  every state delta matched");
        } else {
            for line in &self.differences {
                let _ = writeln!(out, "  {line}");
            }
        }
        out
    }

    /// The produced deltas as an `expect_changes.json`.
    pub fn expectations_json(&self) -> Option<String> {
        serde_json::to_string_pretty(&Expectations { changes: self.produced.clone() }).ok()
    }
}

/// Methods a test core cannot honour, and the reason to print.
fn unreplayable(method: &str) -> Option<&'static str> {
    match method {
        m if m.starts_with("output.") => {
            Some("a test core has no outputs, and a replay must not publish to a real CDN")
        }
        "core.shutdown" => Some("the replay ends by itself"),
        "plugin.add" | "plugin.remove" | "plugin.update" => {
            Some("a replay does not install software")
        }
        "media.upload" | "media.convert" => Some("the media library is not part of a replay"),
        _ => None,
    }
}

/// Replay one session log against a test core.
pub async fn replay(file: &Path, options: &Options) -> Result<Outcome> {
    let records = read(file)?;
    let expected = match &options.expect {
        Some(path) => Expectations::load(path)?.changes,
        None => delta::of(&records),
    };
    let commands: Vec<&Record> = records.iter().filter(|r| r.kind == "command").collect();
    anyhow::ensure!(
        !commands.is_empty(),
        "{} has no command records in it, so there is nothing to replay. A session log records \
         commands as they are accepted; check the range you copied.",
        file.display()
    );

    let started = std::time::Instant::now();
    let core = core::TestCore::start_with(&options.with_plugins).await?;
    let collector = delta::Collector::start(core.watch());
    let mut issued = 0usize;
    let mut skipped = Vec::new();
    let mut errors = Vec::new();
    let mut previous: Option<&str> = None;

    for record in &commands {
        let Some(method) = record.method() else { continue };
        if let Some(why) = unreplayable(method) {
            skipped.push((method.to_string(), why.to_string()));
            continue;
        }
        if !options.fast {
            if let Some(then) = previous {
                let wait = gap_ms(then, &record.ts).min(30_000);
                if wait > 0 {
                    tokio::time::sleep(Duration::from_millis(wait)).await;
                }
            }
        }
        previous = Some(&record.ts);
        let params = rewrite(method, record.params(), options);
        match core.call(method, params).await {
            Ok(_) => issued += 1,
            Err(e) => {
                issued += 1;
                errors.push(format!("{method} answered {}: {}", e.code, e.message));
            }
        }
    }
    // The last commands' events are still on their way; a take lands on the
    // next frame boundary and a source goes live when it produces one.
    core.settle().await;
    let (produced, missed) = collector.finish();
    if missed > 0 {
        errors.push(format!(
            "the replay fell {missed} event(s) behind and the comparison has a hole in it.              Run it again on a quieter machine."
        ));
    }
    drop(core);

    let differences = delta::diff(&expected, &produced);
    Ok(Outcome {
        file: file.to_path_buf(),
        issued,
        skipped,
        errors,
        expected,
        produced,
        differences,
        took: started.elapsed(),
    })
}

/// Make a recorded command deterministic.
///
/// Every source becomes colour bars, or the fixture the caller named. That is
/// what turns "the NDI camera in the hall" into something a laptop with no
/// network can run, and it is why the test doubles exist (10 section 4).
fn rewrite(method: &str, mut params: Value, options: &Options) -> Value {
    let Some(object) = params.as_object_mut() else { return params };
    match method {
        "source.add" => {
            let replacement = match &options.source_fixture {
                Some(path) => format!("file://{}", path.display()),
                None => "test://smpte".to_string(),
            };
            // A `test://` source is already deterministic. A `file://` one is
            // the fixture a support bundle carries, and a replay that threw it
            // away would be replaying a different session.
            let uri = object.get("uri").and_then(Value::as_str).unwrap_or("").to_string();
            let file = uri.strip_prefix("file://");
            if uri.starts_with("test://") || loopback(&uri) {
                // Already deterministic: colour bars, or an address on this
                // machine that nothing is listening on and nothing will be.
            } else if file.is_some_and(|p| Path::new(p).exists()) {
                // The fixture a support bundle carries. Throwing it away would
                // be replaying a different session.
            } else if file.is_some() {
                // A file that was there on the night and is not here. The
                // stand in runs out after two seconds the way a clip does,
                // which is the case worth keeping.
                match stand_in_clip() {
                    Some(clip) => {
                        object.insert("uri".into(), json!(format!("file://{}", clip.display())));
                    }
                    None => {
                        object.insert("uri".into(), json!(replacement));
                    }
                }
                object.remove("type");
                object.remove("params");
            } else {
                object.insert("uri".into(), json!(replacement));
                object.remove("type");
                object.remove("params");
            }
        }
        "adbreak.start" => {
            // The clip that rolled on the night is not in the corpus, and a
            // repository is not where two seconds of video belongs. The replay
            // makes its own instead, so the break still arms, rolls and ends.
            let there =
                object.get("uri").and_then(Value::as_str).is_some_and(|u| Path::new(u).exists());
            if !there {
                if let Some(clip) = stand_in_clip() {
                    object.insert("uri".into(), json!(clip.display().to_string()));
                }
            }
        }
        _ => {}
    }
    params
}

/// Whether a URI points at this machine, and so behaves the same everywhere.
///
/// A session that recorded a camera failing to connect replays as a camera
/// failing to connect, which is the point. Anything pointing outward is
/// replaced, because a replay must not need a network.
fn loopback(uri: &str) -> bool {
    let Some(rest) = uri.split("://").nth(1) else { return false };
    let host = rest.split(['/', ':', '?']).next().unwrap_or("");
    matches!(host, "127.0.0.1" | "localhost" | "[::1]" | "")
        || rest.starts_with("[::1]")
}

/// A two second clip, made once per process, for an ad break whose own media
/// is not on this machine.
///
/// Encoded with what `gst-plugins-base` and `-good` always carry, tried in
/// order, because a replay has to run on a contributor's laptop and on a CI
/// runner with no libav. `None` means none of them worked, and then the ad
/// break is replayed with its original URI and fails the way it would have.
fn stand_in_clip() -> Option<PathBuf> {
    use std::sync::OnceLock;
    static CLIP: OnceLock<Option<PathBuf>> = OnceLock::new();
    CLIP.get_or_init(|| {
        let path = std::env::temp_dir().join("gmx-replay-ad.mkv");
        if path.exists() {
            return Some(path);
        }
        // The sink is named and its `location` is set afterwards, never
        // written into the description. `gst_parse_launch` reads a backslash
        // as an escape, so a Windows temporary directory embedded here comes
        // out as C:UsersRUNNER~1... and the clip is written somewhere nobody
        // then looks. `capture-common`'s wiring.rs takes the same care with
        // socket paths for the same reason.
        let recipes = [
            "videotestsrc num-buffers=60 pattern=smpte ! video/x-raw,width=320,height=180,framerate=30/1 !              videoconvert ! theoraenc ! matroskamux name=m ! filesink name=out              audiotestsrc num-buffers=94 ! audioconvert ! vorbisenc ! m.",
            "videotestsrc num-buffers=60 pattern=smpte ! video/x-raw,width=320,height=180,framerate=30/1 !              videoconvert ! jpegenc ! avimux ! filesink name=out",
        ];
        for recipe in recipes {
            if run_pipeline_writing(recipe, &path).is_ok() && path.exists() {
                return Some(path);
            }
            let _ = std::fs::remove_file(&path);
        }
        tracing::warn!(
            "no encoder on this machine could make a stand in clip, so an ad break replays              with the URI it was recorded with"
        );
        None
    })
    .clone()
}

/// Run one `gst-launch` style description to completion, with the file it
/// writes given to the sink named `out` as a property rather than as text in
/// the description.
fn run_pipeline_writing(description: &str, out: &Path) -> Result<()> {
    use gstreamer::prelude::*;
    let pipeline = gstreamer::parse::launch(description)?;
    let bin = pipeline
        .downcast_ref::<gstreamer::Bin>()
        .context("the clip description did not parse to a bin")?;
    let sink = bin.by_name("out").context("the clip description has no sink named out")?;
    sink.set_property("location", out.to_string_lossy().to_string());
    pipeline.set_state(gstreamer::State::Playing)?;
    let bus = pipeline.bus().context("a pipeline with no bus")?;
    let outcome = bus.timed_pop_filtered(
        gstreamer::ClockTime::from_seconds(30),
        &[gstreamer::MessageType::Eos, gstreamer::MessageType::Error],
    );
    let _ = pipeline.set_state(gstreamer::State::Null);
    let Some(message) = outcome else {
        anyhow::bail!("the clip pipeline neither finished nor failed");
    };
    match message.view() {
        gstreamer::MessageView::Eos(_) => Ok(()),
        gstreamer::MessageView::Error(e) => anyhow::bail!("{}", e.error()),
        _ => anyhow::bail!("the clip pipeline said something unexpected"),
    }
}

/// Every session log in a directory, for the corpus test.
pub fn corpus(dir: &Path) -> Result<BTreeMap<String, PathBuf>> {
    let mut found = BTreeMap::new();
    let entries = std::fs::read_dir(dir)
        .with_context(|| format!("there is no session corpus at {}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            let name = path.file_stem().unwrap_or_default().to_string_lossy().to_string();
            found.insert(name, path);
        }
    }
    Ok(found)
}

/// Where a session log's expectations live: `takes.jsonl` is expected by
/// `takes.expect_changes.json` beside it, or by `expect_changes.json` in the
/// same directory when a whole directory is one case.
pub fn expectations_for(log: &Path) -> Option<PathBuf> {
    let stem = log.file_stem()?.to_string_lossy().to_string();
    let beside = log.with_file_name(format!("{stem}.expect_changes.json"));
    if beside.exists() {
        return Some(beside);
    }
    let in_dir = log.parent()?.join("expect_changes.json");
    in_dir.exists().then_some(in_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_is_read_a_line_at_a_time_and_a_broken_line_is_skipped() {
        let text = concat!(
            r#"{"seq":0,"ts":"2026-01-01T20:13:00.000Z","kind":"command","method":"program.take"}"#,
            "\n",
            "not json at all\n",
            r#"{"seq":1,"ts":"2026-01-01T20:13:01.000Z","kind":"event","event":{"type":"took"}}"#,
            "\n",
        );
        let records = parse(text);
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].method(), Some("program.take"));
        assert_eq!(records[1].event_type(), Some("took"));
    }

    #[test]
    fn the_gap_between_two_timestamps_is_the_timing_a_replay_keeps() {
        assert_eq!(gap_ms("2026-01-01T20:13:00.000Z", "2026-01-01T20:13:01.250Z"), 1_250);
        assert_eq!(gap_ms("2026-01-01T20:13:00.000Z", "2026-01-01T20:14:00.000Z"), 60_000);
        // Out of order, or unparseable, is no wait at all rather than a panic.
        assert_eq!(gap_ms("2026-01-01T20:13:01.000Z", "2026-01-01T20:13:00.000Z"), 0);
        assert_eq!(gap_ms("nonsense", "2026-01-01T20:13:00.000Z"), 0);
    }

    #[test]
    fn a_real_camera_becomes_colour_bars_and_a_test_source_is_left_alone() {
        let options = Options::default();
        let rewritten = rewrite(
            "source.add",
            json!({ "id": "cam1", "uri": "rtmp://hall.example/live/cam1" }),
            &options,
        );
        assert_eq!(rewritten["uri"], "test://smpte");
        let kept =
            rewrite("source.add", json!({ "id": "cam1", "uri": "test://ball" }), &options);
        assert_eq!(kept["uri"], "test://ball");
        // A fixture wins over colour bars.
        let with_fixture = Options {
            source_fixture: Some(PathBuf::from("/tmp/cam1.mkv")),
            ..Options::default()
        };
        let fixed =
            rewrite("source.add", json!({ "id": "cam1", "uri": "ndi://hall" }), &with_fixture);
        assert_eq!(fixed["uri"], "file:///tmp/cam1.mkv");
        // Anything that is not source.add is untouched.
        let take = rewrite("program.take", json!({ "source": "cam1" }), &options);
        assert_eq!(take["source"], "cam1");
    }

    #[test]
    fn a_loopback_address_is_kept_because_it_fails_the_same_way_everywhere() {
        assert!(loopback("rtmp://127.0.0.1:1/live/hall"));
        assert!(loopback("srt://localhost:9000"));
        assert!(!loopback("rtmp://hall.example/live/cam1"));
        assert!(!loopback("ndi://STUDIO (CAM 1)"));
        let kept = rewrite(
            "source.add",
            json!({ "id": "hall", "uri": "rtmp://127.0.0.1:1/live/hall" }),
            &Options::default(),
        );
        assert_eq!(kept["uri"], "rtmp://127.0.0.1:1/live/hall");
    }

    #[test]
    fn the_methods_a_test_core_cannot_honour_say_why() {
        assert!(unreplayable("output.add").unwrap().contains("no outputs"));
        assert!(unreplayable("plugin.add").is_some());
        assert!(unreplayable("program.take").is_none());
        assert!(unreplayable("source.add").is_none());
    }
}
