//! Snapshot cameras: one JPEG per request, asked for `fps` times a second
//! on a thread of its own, pushed into the Matroska the core reads.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gstreamer as gst;
use gstreamer::prelude::*;

use crate::fetch::fetch;
use crate::settings::Settings;

pub fn snapshot_src(pipeline: &gst::Pipeline, s: &Settings, head: &gst::Element) -> Result<gstreamer_app::AppSrc, String> {
    let caps = gst::Caps::builder("image/jpeg").field("framerate", gst::Fraction::new(s.fps as i32, 1)).build();
    let src = gstreamer_app::AppSrc::builder().name("in").caps(&caps).is_live(true).do_timestamp(true).format(gst::Format::Time).build();
    pipeline.add(&src).map_err(|e| e.to_string())?;
    src.link(head).map_err(|e| format!("could not link the snapshots to the muxer: {e}"))?;
    Ok(src)
}

/// Fetch a picture every 1/fps and push it; a failed fetch is remembered for
/// health and skipped.
pub fn poll(s: Settings, src: gstreamer_app::AppSrc, stop: Arc<AtomicBool>, last: Arc<Mutex<Option<String>>>) -> std::thread::JoinHandle<()> {
    let every = Duration::from_millis(1000 / u64::from(s.fps.max(1)));
    std::thread::Builder::new()
        .name("gmx-ipcam-snapshot".into())
        .spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let started = std::time::Instant::now();
                let got = fetch(&s.uri, &s.user, &s.password, Duration::from_secs(3));
                *last.lock().unwrap_or_else(|e| e.into_inner()) = got.as_ref().err().cloned();
                if let Ok(jpeg) = got {
                    if src.push_buffer(gst::Buffer::from_slice(jpeg)).is_err() {
                        break;
                    }
                }
                std::thread::sleep(every.saturating_sub(started.elapsed()));
            }
        })
        .expect("could not start the snapshot thread")
}
