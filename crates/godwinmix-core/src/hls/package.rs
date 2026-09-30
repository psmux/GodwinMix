//! The GStreamer half: one encoded pad in, CMAF fragments into a track's
//! ring out.
//!
//! ```text
//! encoded pad ─▶ queue (leaky, 2 s) ─▶ parser ─▶ cmafmux ─▶ appsink ─▶ Cutter ─▶ Track
//! ```
//!
//! The parser is chosen when the first caps arrive, so the caller does not
//! have to say what codec it is handing over. `cmafmux` cuts a fragment at the
//! first keyframe after `segment_ms` and a chunk every `part_ms`; it never
//! asks upstream for a keyframe (`send-force-keyunit` is off), because the
//! encoders are shared and keeping a ladder's keyframes aligned is the
//! encoders' job. The appsink's callback copies each buffer once into the
//! part being built and returns: nothing on this thread waits for a viewer.

use super::cutter::{Cutter, Piece};
use super::stream::Stream;
use super::parse::{link_parser_on_caps, watch_caps};
use super::track::TrackKind;
use crate::gstutil::{self, make};
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::Arc;

/// One encoded rendition to package.
pub struct Input<'a> {
    /// The rung's slug in URLs: `720p`, `audio`.
    pub id: &'a str,
    pub kind: TrackKind,
    /// An unlinked src pad in the pipeline, carrying H.264, HEVC, AV1, AAC
    /// or Opus. A tee's request pad is the usual thing.
    pub pad: &'a gst::Pad,
    /// What the rendition asked for, for `BANDWIDTH` before anything has
    /// been measured. 0 when unknown.
    pub declared_kbps: u32,
}

/// A packaged pad, kept so it can be taken away again.
pub struct Attached {
    pub track: String,
    tag: String,
    peer: gst::Pad,
    elements: Vec<gst::Element>,
}

/// Package `input` into a new track of `stream`, in `pipeline`.
pub fn attach(pipeline: &gst::Pipeline, stream: &Arc<Stream>, input: Input<'_>) -> Result<Attached> {
    let p = stream.params;
    let tag = format!("hls-{}-{}", stream.id, input.id);
    let track = stream.add_track(input.id, input.kind, input.declared_kbps);
    let queue = gstutil::queue_time(&format!("{tag}-q"), 2.0, true)?;
    let mux = make("cmafmux", &format!("{tag}-mux"))?;
    let (fragment, chunk) = p.mux_durations();
    mux.set_property("fragment-duration", gst::ClockTime::from_nseconds(fragment));
    if let Some(chunk) = chunk {
        mux.set_property("chunk-duration", gst::ClockTime::from_nseconds(chunk));
    }
    mux.set_property("send-force-keyunit", false);
    let sink = make("appsink", &format!("{tag}-sink"))?;
    let appsink = sink.clone().downcast::<gst_app::AppSink>().map_err(|_| anyhow::anyhow!("appsink is not an AppSink"))?;
    appsink.set_sync(false);
    appsink.set_async(false);
    appsink.set_property("enable-last-sample", false);
    let cutter = Cutter::new(track.clone(), input.kind == TrackKind::Audio, !p.low_latency());
    appsink.set_callbacks(callbacks(cutter));

    pipeline.add_many([&queue, &mux, &sink]).context("adding the HLS packager")?;
    mux.link(&sink).context("linking cmafmux to its appsink")?;
    watch_caps(&mux, track);
    link_parser_on_caps(&queue, &mux, &tag)?;
    for el in [&sink, &mux, &queue] {
        el.sync_state_with_parent().ok();
    }
    let qsink = queue.static_pad("sink").context("queue has no sink pad")?;
    input.pad.link(&qsink).with_context(|| format!("linking rendition {} into the HLS packager", input.id))?;
    Ok(Attached { track: input.id.to_string(), tag, peer: input.pad.clone(), elements: vec![queue, mux, sink] })
}

impl Attached {
    /// Take the packager out of `pipeline` and the track out of `stream`.
    /// The caller releases its own pad afterwards.
    pub fn detach(mut self, pipeline: &gst::Pipeline, stream: &Stream) {
        if let Some(qsink) = self.elements.first().and_then(|q| q.static_pad("sink")) {
            let _ = self.peer.unlink(&qsink);
        }
        // The parser was put in when the caps arrived, so it is found by name.
        self.elements.extend(pipeline.by_name(&format!("{}-parse", self.tag)));
        for el in self.elements.iter().rev() {
            let _ = el.set_state(gst::State::Null);
        }
        let _ = pipeline.remove_many(&self.elements);
        stream.remove_track(&self.track);
    }
}

fn callbacks(cutter: Cutter) -> gst_app::AppSinkCallbacks {
    let cutter = Arc::new(Mutex::new(cutter));
    let on_eos = cutter.clone();
    gst_app::AppSinkCallbacks::builder()
        .new_sample(move |sink| {
            let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
            let Some(buf) = sample.buffer() else { return Ok(gst::FlowSuccess::Ok) };
            let piece = piece_of(sink, &sample, buf);
            let map = buf.map_readable().map_err(|_| gst::FlowError::Error)?;
            cutter.lock().push(&map, piece);
            Ok(gst::FlowSuccess::Ok)
        })
        .eos(move |_| on_eos.lock().eos())
        .build()
}

fn piece_of(sink: &gst_app::AppSink, sample: &gst::Sample, buf: &gst::BufferRef) -> Piece {
    let flags = buf.flags();
    let running = sample
        .segment()
        .and_then(|s| s.downcast_ref::<gst::format::Time>())
        .and_then(|s| buf.pts().or(buf.dts()).and_then(|t| s.to_running_time(t)))
        .map(|t| t.nseconds());
    // The wall clock at this buffer's running time: now, less however long
    // ago the pipeline says that was.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let behind_ms = match (sink.current_running_time(), running) {
        (Some(now), Some(at)) => (now.nseconds().saturating_sub(at) / 1_000_000) as i64,
        _ => 0,
    };
    Piece {
        header: flags.contains(gst::BufferFlags::HEADER),
        delta: flags.contains(gst::BufferFlags::DELTA_UNIT),
        marker: flags.contains(gst::BufferFlags::MARKER),
        running_ns: running,
        duration_ns: buf.duration().map(|d| d.nseconds()),
        wall_ms: now_ms - behind_ms,
    }
}
