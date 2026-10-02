//! The probe on the compositor's src pad that puts the board's pictures on
//! each programme frame.
//!
//! It runs on the compositor's own streaming thread, after the frame is
//! made and before anything downstream sees it. Everything it does is bounded
//! arithmetic on memory it already has: no lock is held longer than it takes
//! to copy a list of pads, nothing waits, nothing is rendered. A frame with
//! nothing on the board on air passes through untouched and is not even
//! mapped.

use super::blend::{self, Draw, Planes, Source};
use super::board::Board;
use super::picture::Picture;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;
use std::sync::{Arc, Weak};
use tracing::warn;

/// One picture to put on this frame.
pub struct Job {
    pub z: u32,
    pub picture: Arc<Picture>,
    pub draw: Draw,
}

/// Every job for one frame, in stacking order.
#[derive(Default)]
pub struct Jobs(pub Vec<Job>);

/// Put the drawing probe on the compositor's src pad.
pub(super) fn install(board: &Arc<Board>) -> Option<gst::PadProbeId> {
    let pad = board.compositor().static_pad("src")?;
    let weak: Weak<Board> = Arc::downgrade(board);
    let warned = std::sync::atomic::AtomicBool::new(false);
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(board) = weak.upgrade() else { return gst::PadProbeReturn::Ok };
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
        let now = buffer.pts().map(|t| t.nseconds()).unwrap_or(0);
        let jobs = board.jobs(now);
        if jobs.0.is_empty() {
            return gst::PadProbeReturn::Ok;
        }
        if let Err(why) = paint(pad, buffer.make_mut(), &jobs) {
            if !warned.swap(true, std::sync::atomic::Ordering::Relaxed) {
                warn!(%why, "transparent sources cannot be drawn on this programme");
            }
        }
        gst::PadProbeReturn::Ok
    })
}

/// Draw every job onto one frame, in place.
pub fn paint(pad: &gst::Pad, buffer: &mut gst::BufferRef, jobs: &Jobs) -> Result<(), String> {
    let caps = pad.current_caps().ok_or("the compositor has no caps yet")?;
    let info = gst_video::VideoInfo::from_caps(&caps).map_err(|e| e.to_string())?;
    paint_with(&info, buffer, jobs)
}

/// The same, for a caller that already knows the frame's layout.
pub fn paint_with(info: &gst_video::VideoInfo, buffer: &mut gst::BufferRef, jobs: &Jobs) -> Result<(), String> {
    if info.format() != gst_video::VideoFormat::I420 {
        return Err(format!(
            "the programme is composited in {:?}, and only the software compositor's I420 is drawn on",
            info.format()
        ));
    }
    let mut frame = gst_video::VideoFrameRef::from_buffer_ref_writable(buffer, info).map_err(|e| e.to_string())?;
    let strides = info.stride();
    let strides = [strides[0] as usize, strides[1] as usize, strides[2] as usize];
    let (width, height) = (info.width() as i32, info.height() as i32);
    let [y, u, v, _] = frame.planes_data_mut();
    let mut planes = Planes { y, u, v, strides, width, height };
    for job in &jobs.0 {
        let Ok(map) = job.picture.buffer.map_readable() else { continue };
        let pic = &job.picture;
        let need = pic.stride * (pic.height as usize).saturating_sub(1) + pic.width as usize * 4;
        if map.len() < need {
            continue;
        }
        blend::draw(&mut planes, &Source { data: &map, stride: pic.stride }, &clamped(&job.draw, pic));
    }
    Ok(())
}

/// A draw whose window cannot read past the picture it names.
fn clamped(d: &Draw, pic: &Picture) -> Draw {
    let mut d = *d;
    d.window.x = d.window.x.clamp(0, pic.width as i32);
    d.window.y = d.window.y.clamp(0, pic.height as i32);
    d.window.w = d.window.w.min(pic.width as i32 - d.window.x);
    d.window.h = d.window.h.min(pic.height as i32 - d.window.y);
    d
}
