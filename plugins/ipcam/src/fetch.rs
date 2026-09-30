//! One HTTP GET, through GStreamer's own HTTP client so https and a camera's
//! basic or digest login work the same as for the MJPEG stream.

use std::time::{Duration, Instant};

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

/// The whole body at `uri`, or why not, within `timeout`.
pub fn fetch(uri: &str, user: &str, password: &str, timeout: Duration) -> Result<Vec<u8>, String> {
    let pipeline = gst::Pipeline::new();
    let src = crate::camera::make("souphttpsrc", "")?;
    src.set_property("location", uri);
    src.set_property("timeout", timeout.as_secs().max(1) as u32);
    if !user.is_empty() {
        src.set_property("user-id", user);
        src.set_property("user-pw", password);
    }
    let sink = AppSink::builder().sync(false).build();
    pipeline.add_many([&src, sink.upcast_ref()]).map_err(|e| e.to_string())?;
    src.link(&sink).map_err(|e| e.to_string())?;
    pipeline.set_state(gst::State::Playing).map_err(|_| format!("could not start fetching {uri}"))?;
    let until = Instant::now() + timeout;
    let mut body = Vec::new();
    let outcome = loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break Err(format!("the camera did not answer {uri} within {} s", timeout.as_secs()));
        }
        match sink.try_pull_sample(gst::ClockTime::from_mseconds(left.as_millis() as u64)) {
            Some(sample) => {
                if let Some(map) = sample.buffer().and_then(|b| b.map_readable().ok()) {
                    body.extend_from_slice(&map);
                }
            }
            None if sink.is_eos() => break Ok(()),
            None => break Err(error_of(&pipeline).unwrap_or_else(|| format!("nothing came from {uri}"))),
        }
    };
    let _ = pipeline.set_state(gst::State::Null);
    outcome?;
    if body.len() < 4 || body[0] != 0xff || body[1] != 0xd8 {
        return Err(format!("{uri} did not answer with a JPEG picture"));
    }
    Ok(body)
}

fn error_of(pipeline: &gst::Pipeline) -> Option<String> {
    let bus = pipeline.bus()?;
    let msg = bus.pop_filtered(&[gst::MessageType::Error])?;
    match msg.view() {
        gst::MessageView::Error(e) => Some(e.error().to_string()),
        _ => None,
    }
}
