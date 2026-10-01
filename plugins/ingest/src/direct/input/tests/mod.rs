//! Every input against a real sender on this machine: ffmpeg or
//! gst-launch-1.0 sending, an input receiving, and what came out checked by
//! ffprobe reading it back as FLV. Ports 19921 to 19940. Each test skips,
//! saying so, when a tool it needs is not installed, and every wait has a
//! deadline.

mod files;
mod pull;
mod ts;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::super::{StopSignal, TagSink};
use super::{open, Context, InputSpec, InputStats};
use crate::media_tag::{MediaTag, TagKind};

#[derive(Default)]
pub struct Got {
    pub tags: Vec<MediaTag>,
    pub stats: Vec<InputStats>,
}

#[derive(Clone, Default)]
pub struct Collect(pub Arc<Mutex<Got>>);

impl TagSink for Collect {
    fn tag(&mut self, tag: MediaTag) {
        self.0.lock().unwrap().tags.push(tag);
    }
    fn stats(&mut self, stats: &InputStats) {
        self.0.lock().unwrap().stats.push(stats.clone());
    }
}

impl Collect {
    pub fn frames(&self, kind: TagKind) -> usize {
        self.0.lock().unwrap().tags.iter().filter(|t| t.kind == kind && !t.sequence_header).count()
    }
    pub fn keyframes(&self) -> usize {
        self.0.lock().unwrap().tags.iter().filter(|t| t.keyframe && !t.sequence_header).count()
    }
    pub fn last(&self) -> InputStats {
        self.0.lock().unwrap().stats.last().cloned().unwrap_or_default()
    }
    pub fn times(&self) -> Vec<u32> {
        self.0.lock().unwrap().tags.iter().filter(|t| t.kind == TagKind::Video).map(|t| t.timestamp_ms).collect()
    }
}

/// An input running on its own thread, stopped and joined when dropped.
pub struct Running {
    pub got: Collect,
    stop: StopSignal,
    thread: Option<std::thread::JoinHandle<()>>,
}

pub fn start(spec: Value, ctx: &Context) -> Running {
    let spec = InputSpec::from_json(&spec).expect("a good spec");
    let input = open(&spec, ctx).unwrap_or_else(|e| panic!("{e}"));
    let (got, stop) = (Collect::default(), StopSignal::default());
    let (sink, s) = (Box::new(got.clone()), stop.clone());
    let thread = std::thread::spawn(move || input.run(sink, s));
    Running { got, stop, thread: Some(thread) }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.stop.stop();
        let started = Instant::now();
        if let Some(t) = self.thread.take() {
            t.join().ok();
        }
        assert!(started.elapsed() < Duration::from_secs(5), "the input took {:?} to stop", started.elapsed());
    }
}

/// Wait up to `secs` for `done`, polling.
pub fn eventually(secs: u64, mut done: impl FnMut() -> bool) -> bool {
    let until = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < until {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    done()
}

pub fn which(tool: &str) -> bool {
    let found = std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(tool).is_file()));
    if !found {
        eprintln!("skipping: needs {tool} on PATH");
    }
    found
}

/// A child process killed when dropped.
pub struct Sender(pub Child);

impl Drop for Sender {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn spawn(tool: &str, args: &[&str]) -> Sender {
    let child = Command::new(tool).args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
    Sender(child.unwrap_or_else(|e| panic!("{tool} would not start: {e}")))
}

pub fn gst(line: &str) -> Sender {
    let args: Vec<&str> = std::iter::once("-q").chain(line.split_whitespace()).collect();
    spawn("gst-launch-1.0", &args)
}

/// A live H.264 and AAC encode in MPEG-TS, seven packets a buffer, into
/// `sink` (a gst-launch fragment). Paced by its live sources, so it sends
/// as a hardware encoder would.
pub fn live_ts(sink: &str) -> String {
    format!(
        "videotestsrc is-live=true ! video/x-raw,width=320,height=240,framerate=25/1 ! x264enc tune=zerolatency \
         speed-preset=ultrafast key-int-max=25 ! h264parse ! queue ! mpegtsmux name=m alignment=7 ! {sink} \
         audiotestsrc is-live=true ! audioconvert ! avenc_aac ! aacparse ! queue ! m."
    )
}

/// A pipeline run in this process, stopped when dropped.
pub struct Local(pub gstreamer::Pipeline);

impl Local {
    pub fn launch(line: &str) -> Local {
        use gstreamer::prelude::*;
        gmx_netkit::init().unwrap();
        let p = gstreamer::parse::launch(line).unwrap().downcast::<gstreamer::Pipeline>().unwrap();
        p.set_state(gstreamer::State::Playing).unwrap();
        Local(p)
    }
}

impl Drop for Local {
    fn drop(&mut self) {
        use gstreamer::prelude::*;
        let _ = self.0.set_state(gstreamer::State::Null);
    }
}

/// Write what came out as FLV and ask ffprobe what it decodes. Answers
/// `(codec, frames, width, height, channels)` per stream.
pub fn probe(got: &Collect, name: &str) -> Vec<(String, u64, u64, u64, u64)> {
    let path = scratch().join(format!("{name}.flv"));
    let mut bytes = crate::flv::header();
    for tag in &got.0.lock().unwrap().tags {
        bytes.extend(crate::flv::write(tag));
    }
    std::fs::write(&path, bytes).unwrap();
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-count_frames", "-show_entries", "stream=codec_name,nb_read_frames,width,height,channels", "-of", "json"])
        .arg(&path)
        .output()
        .expect("ffprobe runs");
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let n = |s: &Value, k: &str| s[k].as_u64().or_else(|| s[k].as_str().and_then(|t| t.parse().ok())).unwrap_or(0);
    v["streams"].as_array().cloned().unwrap_or_default().iter()
        .map(|s| (s["codec_name"].as_str().unwrap_or("").to_string(), n(s, "nb_read_frames"), n(s, "width"), n(s, "height"), n(s, "channels")))
        .collect()
}

pub fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-directin-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

static CLIPS: Mutex<()> = Mutex::new(());
static SENDERS: Mutex<()> = Mutex::new(());

/// One live sender at a time: six encoders at once on a busy machine lose
/// datagrams in the kernel before the input sees them, which is the machine
/// and not the input.
pub fn one_at_a_time() -> std::sync::MutexGuard<'static, ()> {
    SENDERS.lock().unwrap_or_else(|e| e.into_inner())
}

/// A clip made once per test run with ffmpeg: `inputs` then `-t secs`, `out`.
pub fn clip(name: &str, args: &[&str]) -> PathBuf {
    let _one = CLIPS.lock().unwrap_or_else(|e| e.into_inner());
    let path = scratch().join(name);
    if !path.is_file() {
        let ok = Command::new("ffmpeg").args(["-y", "-loglevel", "error"]).args(args).arg(&path).status().unwrap().success();
        assert!(ok, "ffmpeg could not make {name}");
    }
    path
}

/// A 320x240 25 fps H.264 and AAC clip in MPEG-TS, `secs` long.
pub fn ts_clip(secs: u32) -> PathBuf {
    let t = secs.to_string();
    clip(&format!("h264-aac-{secs}.ts"), &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440",
        "-t", &t, "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac", "-f", "mpegts"])
}

pub fn path(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}
