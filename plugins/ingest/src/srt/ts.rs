//! One SRT caller's MPEG-TS, demuxed into the hub's tags.
//!
//! ```text
//!   appsrc ──► tsdemux ─┬─► queue ──► h264parse ──► appsink (video tags)
//!                       └─► queue ──► aacparse ──► appsink (audio tags)
//! ```
//!
//! A demux and two parsers, and nothing decoded: the bytes an encoder sent
//! are the bytes a reader of the hub gets, framed the way RTMP frames them.
//! A stream in any other codec (HEVC, MP2 audio) is said so once and its
//! pads go to a fakesink, because the hub carries what FLV can.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

use crate::rtmp::Inlet;
use crate::tagger::{self, Shared, Zero};

/// The ceiling on bytes waiting for the demuxer. A second of a 20 Mbit/s
/// stream; past it the oldest bytes go, and the publisher never waits.
const MAX_BYTES: u64 = 2_500_000;

pub struct Demux {
    pipe: Pipe,
    src: gst_app::AppSrc,
    to: Shared,
}

impl Demux {
    pub fn start(inlet: Box<dyn Inlet>, reporter: Option<Reporter>, name: &str) -> Result<Demux, String> {
        gmx_netkit::init()?;
        let to = tagger::share(inlet);
        let pipeline = gst::Pipeline::with_name(&format!("srt-{name}"));
        let caps = gst::Caps::builder("video/mpegts").field("systemstream", true).field("packetsize", 188i32).build();
        let src = gst_app::AppSrc::builder()
            .caps(&caps)
            .is_live(true)
            .format(gst::Format::Bytes)
            .max_bytes(MAX_BYTES)
            .block(false)
            .build();
        src.set_property_from_str("leaky-type", "downstream");
        let demux = gst::ElementFactory::make("tsdemux").build().map_err(|e| format!("tsdemux is missing: {e}"))?;
        pipeline.add_many([src.upcast_ref(), &demux]).map_err(|e| e.to_string())?;
        src.link(&demux).map_err(|e| e.to_string())?;
        let zero = Arc::new(Zero::default());
        let told = Arc::new(AtomicBool::new(false));
        let (weak, shared) = (pipeline.downgrade(), to.clone());
        demux.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = attach(&pipeline, pad, &shared, &zero) {
                if let (false, Some(r)) = (told.swap(true, Ordering::Relaxed), &reporter) {
                    r.warn(e);
                }
            }
        });
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(None)?;
        Ok(Demux { pipe, src, to })
    }

    /// Hand bytes to the demuxer. Never waits.
    pub fn push(&self, bytes: &[u8]) {
        let _ = self.src.push_buffer(gst::Buffer::from_slice(bytes.to_vec()));
    }

    pub fn failure(&self) -> Option<String> {
        self.pipe.failure()
    }
}

impl Drop for Demux {
    fn drop(&mut self) {
        self.pipe.stop();
        tagger::close(&self.to);
    }
}

/// One elementary stream out of the demuxer, to the sink for its kind.
fn attach(pipeline: &gst::Pipeline, pad: &gst::Pad, to: &Shared, zero: &Arc<Zero>) -> Result<(), String> {
    let caps = pad.current_caps().unwrap_or_else(|| pad.query_caps(None));
    let s = caps.structure(0).map(|s| s.to_owned());
    let kind = s.as_ref().map(|s| s.name().to_string()).unwrap_or_default();
    let mpeg = s.as_ref().and_then(|s| s.get::<i32>("mpegversion").ok()).unwrap_or(0);
    let (parser, sink, carried) = match kind.as_str() {
        "video/x-h264" => ("h264parse", tagger::video_sink(to.clone(), zero.clone()), true),
        "audio/mpeg" if mpeg == 2 || mpeg == 4 => ("aacparse", tagger::audio_sink(to.clone(), zero.clone()), true),
        _ => ("identity", make("fakesink")?, false),
    };
    let queue = make("queue")?;
    let parse = make(parser)?;
    pipeline.add_many([&queue, &parse, &sink]).map_err(|e| e.to_string())?;
    gst::Element::link_many([&queue, &parse, &sink]).map_err(|e| e.to_string())?;
    for e in [&queue, &parse, &sink] {
        let _ = e.sync_state_with_parent();
    }
    let into = queue.static_pad("sink").ok_or("a queue with no sink pad")?;
    pad.link(&into).map_err(|e| format!("{kind} would not link: {e:?}"))?;
    if carried {
        Ok(())
    } else {
        Err(format!(
            "an SRT stream carries {kind}, which a channel does not carry yet. Channels take \
             H.264 video and AAC audio; set the encoder to those."
        ))
    }
}

fn make(factory: &str) -> Result<gst::Element, String> {
    gst::ElementFactory::make(factory).build().map_err(|e| format!("GStreamer has no {factory}: {e}"))
}
