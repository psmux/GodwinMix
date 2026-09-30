//! The feeder pipeline: the core's Matroska off the FIFO, demuxed and parsed
//! into the `Feed`'s appsinks. It is built once per start and never touches
//! a client.

use std::sync::Arc;

use gmx_netkit::pipe::Pipe;
use godwinmix_capture_common::fifo::{Fifo, Pump};
use godwinmix_sdk::plugin::Reporter;
use gstreamer as gst;
use gstreamer::prelude::*;

use crate::feed::{Feed, Kind, Track};

pub const NEEDED: &[&str] = &["appsrc", "matroskademux", "appsink", "rtph264pay", "rtpmp4gpay"];

pub struct Ingest {
    pub pipe: Pipe,
    pump: Option<Pump>,
}

impl Ingest {
    /// Start reading `fifo`. `ready` is called, from a streaming thread and
    /// without waiting on anything, once the demuxer has found every stream.
    pub fn start(fifo: Fifo, feed: Arc<Feed>, ready: Box<dyn Fn() + Send + Sync>, reporter: Option<Reporter>) -> Result<Ingest, String> {
        gmx_netkit::init()?;
        gmx_netkit::elements::require(NEEDED)?;
        let pipeline = gst::Pipeline::with_name("gmx-rtsp-feed");
        let src = make("appsrc", "in")?;
        src.set_property("caps", gst::Caps::new_empty_simple("video/x-matroska"));
        src.set_property_from_str("format", "bytes");
        src.set_property("block", true);
        src.set_property("max-bytes", 4u64 * 1024 * 1024);
        let demux = make("matroskademux", "demux")?;
        pipeline.add_many([&src, &demux]).map_err(|e| format!("could not assemble the feed: {e}"))?;
        src.link(&demux).map_err(|e| format!("could not link the FIFO to the demuxer: {e}"))?;
        let weak = pipeline.downgrade();
        let (f, r) = (feed.clone(), reporter.clone());
        demux.connect_pad_added(move |_, pad| {
            let Some(pipeline) = weak.upgrade() else { return };
            if let Err(e) = branch(&pipeline, pad, &f) {
                if let Some(r) = &r {
                    r.error(format!("a programme stream is left out of the RTSP stream: {e}"));
                }
            }
        });
        demux.connect_no_more_pads(move |_| ready());
        let mut pipe = Pipe::wrap(pipeline);
        pipe.play(reporter)?;
        Ok(Ingest { pipe, pump: Some(Pump::start(fifo, src)) })
    }

    pub fn bytes(&self) -> u64 {
        self.pump.as_ref().map_or(0, Pump::bytes)
    }
}

impl Drop for Ingest {
    fn drop(&mut self) {
        if let Some(mut p) = self.pump.take() {
            p.stop();
        }
        self.pipe.stop();
    }
}

pub fn make(factory: &str, name: &str) -> Result<gst::Element, String> {
    let mut b = gst::ElementFactory::make(factory);
    if !name.is_empty() {
        b = b.name(name);
    }
    b.build().map_err(|_| format!("GStreamer has no {factory}. It comes from {}.", gmx_netkit::elements::where_from(factory)))
}

/// The parser and payloader for a stream, or `None` for one RTSP here does
/// not carry.
pub fn track_for(caps: &gst::Caps) -> Option<(Kind, &'static str, &'static str)> {
    let s = caps.structure(0)?;
    let version = s.get::<i32>("mpegversion").unwrap_or(0);
    Some(match s.name().as_str() {
        "video/x-h264" => (Kind::Video, "h264parse", "rtph264pay"),
        "video/x-h265" => (Kind::Video, "h265parse", "rtph265pay"),
        "audio/mpeg" if version == 4 || version == 2 => (Kind::Audio, "aacparse", "rtpmp4gpay"),
        "audio/mpeg" if version == 1 => (Kind::Audio, "mpegaudioparse", "rtpmpapay"),
        "audio/x-opus" => (Kind::Audio, "opusparse", "rtpopuspay"),
        _ => return None,
    })
}

/// queue ! parser ! appsink for one demuxed stream; a fakesink for one RTSP
/// here does not carry, so the demuxer never stops on "not linked".
fn branch(pipeline: &gst::Pipeline, pad: &gst::Pad, feed: &Arc<Feed>) -> Result<(), String> {
    let caps = pad.current_caps().ok_or("a stream appeared with no caps")?;
    let Some((kind, parse, pay)) = track_for(&caps) else {
        let sink = make("fakesink", "")?;
        pipeline.add(&sink).map_err(|e| e.to_string())?;
        sink.sync_state_with_parent().ok();
        pad.link(&sink.static_pad("sink").ok_or("no sink pad")?).map_err(|e| e.to_string())?;
        let name = caps.structure(0).map(|s| s.name().to_string()).unwrap_or_default();
        return Err(format!("RTSP here carries H.264, H.265, AAC, MP3 and Opus, not {name}"));
    };
    let slot = feed.slot(kind);
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(Track { caps: caps.clone(), parse, pay });
    let queue = make("queue", "")?;
    let parser = make(parse, "")?;
    if parser.find_property("config-interval").is_some() {
        parser.set_property("config-interval", -1i32);
    }
    let sink = feed.sink(kind);
    pipeline.add_many([&queue, &parser, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    gst::Element::link_many([&queue, &parser, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    for e in [&queue, &parser, sink.upcast_ref()] {
        e.sync_state_with_parent().ok();
    }
    pad.link(&queue.static_pad("sink").ok_or("no queue pad")?).map_err(|e| e.to_string())?;
    Ok(())
}
