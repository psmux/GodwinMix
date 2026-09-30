//! The two ways a camera hands over pictures over HTTP, as pipelines that
//! write streamable Matroska to the core.
//!
//! ```text
//!   MJPEG:     souphttpsrc ──► multipartdemux ──► jpegparse ──► matroskamux ──► fdsink fd=1
//!   snapshot:  (a fetch every 1/fps) ──► appsrc ──► jpegparse ──► matroskamux ──► fdsink fd=1
//! ```
//!
//! Nothing is decoded here: the JPEGs cross as they came and the core decodes
//! them once. A snapshot that fails is skipped, and the last picture stays
//! on screen until the next one arrives.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};


use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::snapshot::{poll, snapshot_src};
use crate::settings::{Mode, Settings};

/// Where the Matroska goes: stdout in a running plugin, a file in a test.
#[derive(Debug, Clone)]
pub enum Sink {
    Stdout,
    #[allow(dead_code)]
    File(std::path::PathBuf),
}

pub struct Camera {
    pub pipe: Pipe,
    pub frames: Arc<AtomicU64>,
    pub last_error: Arc<Mutex<Option<String>>>,
    stop: Arc<AtomicBool>,
    poller: Option<std::thread::JoinHandle<()>>,
}

pub fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let mut b = gst::ElementFactory::make(factory);
    if !name.is_empty() {
        b = b.name(name);
    }
    b.build().map_err(|_| format!("GStreamer has no {factory}. It comes from {}.", gmx_netkit::elements::where_from(factory)))
}

/// jpegparse, a queue, the muxer and the sink, linked; the first is returned.
fn tail(pipeline: &gst::Pipeline, sink: &Sink, frames: &Arc<AtomicU64>) -> Result<gst::Element, String> {
    let parse = make("jpegparse", "parse")?;
    let queue = make("queue", "q")?;
    let mux = make("matroskamux", "mux")?;
    mux.set_property("streamable", true);
    let out = match sink {
        Sink::Stdout => {
            let o = make("fdsink", "out")?;
            o.set_property("fd", 1i32);
            o
        }
        Sink::File(p) => {
            let o = make("filesink", "out")?;
            o.set_property("location", p.to_string_lossy().to_string());
            o
        }
    };
    out.set_property("sync", false);
    pipeline.add_many([&parse, &queue, &mux, &out]).map_err(|e| e.to_string())?;
    gst::Element::link_many([&parse, &queue, &mux, &out]).map_err(|e| format!("could not link the muxer: {e}"))?;
    let count = frames.clone();
    if let Some(pad) = parse.static_pad("src") {
        pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
            count.fetch_add(1, Ordering::Relaxed);
            gst::PadProbeReturn::Ok
        });
    }
    Ok(parse)
}

impl Camera {
    pub fn start(s: &Settings, sink: Sink, reporter: Option<Reporter>) -> Result<Camera, String> {
        gmx_netkit::init()?;
        let pipeline = gst::Pipeline::with_name("gmx-ipcam");
        let frames = Arc::new(AtomicU64::new(0));
        let head = tail(&pipeline, &sink, &frames)?;
        let (stop, last_error) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(None)));
        let poller = match s.mode {
            Mode::Mjpeg => {
                mjpeg(&pipeline, s, &head)?;
                None
            }
            Mode::Snapshot => {
                let src = snapshot_src(&pipeline, s, &head)?;
                Some(poll(s.clone(), src, stop.clone(), last_error.clone()))
            }
        };
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter)?;
        Ok(Camera { pipe, frames, last_error, stop, poller })
    }
}

impl Drop for Camera {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.poller.take() {
            let _ = t.join();
        }
        self.pipe.stop();
    }
}

/// souphttpsrc into multipartdemux, whose one pad goes to `head`.
fn mjpeg(pipeline: &gst::Pipeline, s: &Settings, head: &gst::Element) -> Result<(), String> {
    let src = make("souphttpsrc", "src")?;
    src.set_property("location", &s.uri);
    src.set_property("is-live", true);
    src.set_property("do-timestamp", true);
    if !s.user.is_empty() {
        src.set_property("user-id", &s.user);
        src.set_property("user-pw", &s.password);
    }
    let demux = make("multipartdemux", "demux")?;
    pipeline.add_many([&src, &demux]).map_err(|e| e.to_string())?;
    src.link(&demux).map_err(|e| format!("could not link the camera to the demuxer: {e}"))?;
    let target = head.static_pad("sink").ok_or("jpegparse has no sink pad")?;
    demux.connect_pad_added(move |_, pad| {
        if !target.is_linked() {
            let _ = pad.link(&target);
        }
    });
    Ok(())
}
