//! One frame from the middle of a clip, with its alpha, for a preview.
//!
//! The middle rather than the first frame, because a lower third that
//! animates in is empty on its first frame and whole by its middle. A
//! pipeline of its own, run for one frame and taken down, on the blocking
//! thread that asked: never the mixer's.
//!
//! ```text
//!   uridecodebin =| videoconvert -> videoscale -> RGBA at the box -> appsink
//!                \| (any audio) -> fakesink
//! ```

use super::rgba::Layer;
use crate::gallery::place;
use crate::gstutil::make;
use anyhow::{Context, Result};
use godwinmix_protocol::gallery::Zone;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_video as gst_video;
use std::path::Path;

/// How long a clip has to give up a frame.
const WAIT: gst::ClockTime = gst::ClockTime::from_seconds(10);

/// A frame of the clip at `file`, drawn in `zone` on a `canvas` sized preview.
pub fn frame(file: &Path, zone: Zone, canvas: (u32, u32)) -> Result<Layer> {
    let t = place::transform(zone, canvas, None);
    let n = |p: &str| t.pointer(p).and_then(serde_json::Value::as_f64).unwrap_or(0.0);
    let (at, size) = ((n("/position/x") as i32, n("/position/y") as i32), ((n("/frame/w") as u32).max(2) & !1, (n("/frame/h") as u32).max(2) & !1));
    let uri = crate::input::to_uri(&file.display().to_string());
    let sample = grab(&uri, size)?;
    let buffer = sample.buffer().context("a frame with no buffer")?;
    let info = gst_video::VideoInfo::from_caps(sample.caps().context("a frame with no caps")?).context("a frame that is not video")?;
    let map = buffer.map_readable().context("reading the frame")?;
    let stride = info.stride()[0] as usize;
    let mut rgba = Vec::with_capacity((size.0 * size.1 * 4) as usize);
    for y in 0..info.height() as usize {
        rgba.extend_from_slice(&map[y * stride..y * stride + info.width() as usize * 4]);
    }
    Ok(Layer { rgba, width: info.width(), height: info.height(), x: at.0, y: at.1 })
}

fn grab(uri: &str, size: (u32, u32)) -> Result<gst::Sample> {
    let pipeline = gst::Pipeline::new();
    let decode = make("uridecodebin", "gallery-clip")?;
    decode.set_property("uri", uri);
    let convert = make("videoconvert", "gallery-clip-convert")?;
    let scale = make("videoscale", "gallery-clip-scale")?;
    let caps = gst::Caps::builder("video/x-raw").field("format", "RGBA").field("width", size.0 as i32).field("height", size.1 as i32).build();
    let filter = crate::gstutil::capsfilter("gallery-clip-caps", &caps)?;
    let sink = gst_app::AppSink::builder().sync(false).max_buffers(1).build();
    pipeline.add_many([&decode, &convert, &scale, &filter, sink.upcast_ref()]).context("building the clip reader")?;
    gst::Element::link_many([&convert, &scale, &filter, sink.upcast_ref()]).context("linking the clip reader")?;
    let (weak, convert_sink) = (pipeline.downgrade(), convert.static_pad("sink").context("converter pad")?);
    decode.connect_pad_added(move |_, pad| {
        let video = pad.current_caps().and_then(|c| c.structure(0).map(|s| s.name().starts_with("video/"))).unwrap_or(false);
        if video && !convert_sink.is_linked() {
            let _ = pad.link(&convert_sink);
        } else if let (Some(p), Ok(fake)) = (weak.upgrade(), make("fakesink", &format!("gallery-clip-drop-{}", pad.name()))) {
            fake.set_property("sync", false);
            let _ = p.add(&fake);
            let _ = fake.sync_state_with_parent();
            if let Some(sink) = fake.static_pad("sink") {
                let _ = pad.link(&sink);
            }
        }
    });
    let result = run(&pipeline, &sink);
    let _ = pipeline.set_state(gst::State::Null);
    result
}

fn run(pipeline: &gst::Pipeline, sink: &gst_app::AppSink) -> Result<gst::Sample> {
    pipeline.set_state(gst::State::Paused).context("opening the clip")?;
    let first = sink.try_pull_preroll(WAIT).context("the clip gave no frame in ten seconds; is it a video the mixer can play?")?;
    let Some(duration) = pipeline.query_duration::<gst::ClockTime>().filter(|d| *d > gst::ClockTime::from_mseconds(200)) else {
        return Ok(first);
    };
    let middle = duration / 2;
    if pipeline.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, middle).is_err() {
        return Ok(first);
    }
    Ok(sink.try_pull_preroll(WAIT).unwrap_or(first))
}
