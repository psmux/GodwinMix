//! Remux only. The programme encoder and its other consumers remain independent.
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// `parser` is the picture's parser, `h264parse` or `h265parse`. It sits
/// between the programme's encoder and the muxer because `openh264enc`,
/// `mfh264enc` and the hardware encoders send byte-stream, which `mp4mux`
/// and `matroskamux` refuse: the link failed outright on the Windows runner
/// whose programme encoder was one of those. The parser repacks it as AVC,
/// as it already does in front of the RTMP muxer.
pub fn build(
    pipeline: &gst::Pipeline,
    video: &gst::Element,
    audio: &gst::Element,
    parser: &str,
    path: &Path,
    format: &str,
    bytes: Arc<AtomicU64>,
) -> Result<()> {
    let factory = if format == "mkv" {
        "matroskamux"
    } else {
        "mp4mux"
    };
    let mux = gst::ElementFactory::make(factory)
        .build()
        .with_context(|| {
            format!("recording needs {factory}; install the GStreamer good plugins")
        })?;
    if format == "mp4" {
        mux.set_property("fragment-duration", 1000u32);
    }
    let sink = gst::ElementFactory::make("filesink")
        .property("location", path.to_string_lossy().as_ref())
        .property("sync", false)
        .property("async", false)
        .build()?;
    let parse = gst::ElementFactory::make(parser)
        .build()
        .with_context(|| format!("recording needs {parser}; install the GStreamer bad plugins"))?;
    pipeline.add_many([&parse, &mux, &sink])?;
    video
        .link(&parse)
        .context("linking encoded picture to its parser")?;
    crate::plugin::output::link_to_mux(&parse, &mux, &["video_%u"])
        .context("linking encoded picture to the recorder")?;
    audio
        .link(&mux)
        .context("linking encoded sound to the recorder")?;
    mux.link(&sink)?;
    sink.static_pad("sink")
        .context("the recorder has no input")?
        .add_probe(gst::PadProbeType::BUFFER, move |_, info| {
            if let Some(gst::PadProbeData::Buffer(buffer)) = &info.data {
                bytes.fetch_add(buffer.size() as u64, Ordering::Relaxed);
            }
            gst::PadProbeReturn::Ok
        });
    Ok(())
}
