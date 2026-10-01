//! Every input against a real sender on this machine: ffmpeg or
//! gst-launch-1.0 sending, an input receiving, and what came out checked by
//! ffprobe reading it back as FLV. Ports 19921 to 19940. Each test skips,
//! saying so, when a tool it needs is not installed, and every wait has a
//! deadline.

mod cpu;
mod files;
mod pull;
mod restart;
mod tools;
mod ts;

pub use tools::*;

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
