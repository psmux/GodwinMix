//! Remux only. The programme encoder and its other consumers remain independent.
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use std::path::Path;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

pub fn build(
    pipeline: &gst::Pipeline,
    video: &gst::Element,
    audio: &gst::Element,
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
    pipeline.add_many([&mux, &sink])?;
    video
        .link(&mux)
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
