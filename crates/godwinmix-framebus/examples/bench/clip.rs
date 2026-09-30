//! The H.264 clip every scenario decodes, and the pipelines around it.

use std::path::PathBuf;

use gstreamer as gst;
use gstreamer::prelude::*;

/// A 1080p30 H.264 clip of `secs` seconds at 6 Mbit/s with a keyframe every
/// two seconds, made once and kept under the target directory.
pub fn make(secs: u64) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/framebus-bench");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(format!("clip-1080p30-{secs}s.mp4"));
    if path.exists() {
        return path;
    }
    eprintln!("making {} ...", path.display());
    let p = gst::parse::launch(&format!(
        "videotestsrc num-buffers={n} pattern=smpte ! \
         video/x-raw,format=I420,width=1920,height=1080,framerate=30/1 ! \
         timeoverlay font-desc=\"Sans 60\" ! \
         x264enc bitrate=6000 key-int-max=60 speed-preset=veryfast ! h264parse ! \
         mp4mux ! filesink location={loc}",
        n = secs * 30,
        loc = path.display()
    ))
    .unwrap();
    p.set_state(gst::State::Playing).unwrap();
    let bus = p.bus().unwrap();
    for msg in bus.iter_timed(gst::ClockTime::NONE) {
        match msg.view() {
            gst::MessageView::Eos(_) => break,
            gst::MessageView::Error(e) => panic!("making the clip: {}", e.error()),
            _ => {}
        }
    }
    p.set_state(gst::State::Null).unwrap();
    path
}

/// Decode `clip` in real time with `decoder` into `tail`.
pub fn decode_into(clip: &str, decoder: &str, tail: &str) -> gst::Pipeline {
    gst::parse::launch(&format!(
        "filesrc location={clip} ! qtdemux ! h264parse ! {decoder} ! {tail}"
    ))
    .unwrap_or_else(|e| panic!("building the pipeline with {decoder}: {e}"))
    .downcast::<gst::Pipeline>()
    .unwrap()
}

/// Read every cache line of a frame once, as a compositor reading it would.
pub fn touch(bytes: &[u8]) -> u64 {
    bytes
        .iter()
        .step_by(64)
        .fold(0u64, |a, &b| a.wrapping_add(b as u64))
}
