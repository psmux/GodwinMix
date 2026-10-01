//! Frames smaller than the size the camera agreed to.
//!
//! On macOS one camera has one active format for every app that has it open.
//! When a second app opens it at another size (a browser using the same
//! camera as "This browser's camera" is the common case), `avfvideosrc`
//! keeps the caps it negotiated and starts handing over frames at the new
//! size. Every element that maps such a frame fails GStreamer's
//! `info->width <= meta->width` assertion, once a frame, and what it reads is
//! not the picture. So frames whose video meta is smaller than the caps are
//! dropped here, and the plugin says once, on stderr, what is happening and
//! what to do. When the frames match again it says that too.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_video as gst_video;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Drop frames that do not fit the caps on `source`'s output.
pub fn install(source: &gst::Element) {
    let Some(pad) = source.static_pad("src") else { return };
    let dropped = Arc::new(AtomicU64::new(0));
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(buffer) = info.buffer() else { return gst::PadProbeReturn::Ok };
        let Some(meta) = buffer.meta::<gst_video::VideoMeta>() else { return gst::PadProbeReturn::Ok };
        let Some(agreed) = agreed_size(pad) else { return gst::PadProbeReturn::Ok };
        let got = (meta.width(), meta.height());
        if fits(got, agreed) {
            let n = dropped.swap(0, Ordering::Relaxed);
            if n > 0 {
                eprintln!("the camera's frames are {}x{} again; {n} were dropped while they were not", got.0, got.1);
            }
            return gst::PadProbeReturn::Ok;
        }
        if dropped.fetch_add(1, Ordering::Relaxed) == 0 {
            eprintln!("{}", warning(got, agreed));
        }
        gst::PadProbeReturn::Drop
    });
}

/// The width and height of raw video caps on `pad`, or None for anything else.
fn agreed_size(pad: &gst::Pad) -> Option<(u32, u32)> {
    let caps = pad.current_caps()?;
    let s = caps.structure(0)?;
    if s.name() != "video/x-raw" {
        return None;
    }
    let w = s.get::<i32>("width").ok()?;
    let h = s.get::<i32>("height").ok()?;
    Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))
}

/// Can a frame of `got` be read as `agreed`? Only when it is at least as big.
pub fn fits(got: (u32, u32), agreed: (u32, u32)) -> bool {
    got.0 >= agreed.0 && got.1 >= agreed.1
}

pub fn warning(got: (u32, u32), agreed: (u32, u32)) -> String {
    format!(
        "the camera is sending {}x{} frames but agreed to {}x{}. Another app has probably opened \
         the same camera at another size, for example a browser using it as This browser's camera. \
         Those frames are dropped, so the picture holds still until that app lets go of the camera \
         or one of the two sources is removed.",
        got.0, got.1, agreed.0, agreed.1
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_smaller_than_the_caps_does_not_fit() {
        assert!(!fits((1280, 720), (1920, 1080)));
        assert!(!fits((1920, 720), (1920, 1080)));
        assert!(fits((1920, 1080), (1920, 1080)));
        // Padded rows are bigger than the picture, and fine to read.
        assert!(fits((1936, 1088), (1920, 1080)));
    }

    #[test]
    fn the_warning_names_both_sizes_and_the_likely_cause() {
        let w = warning((1280, 720), (1920, 1080));
        assert!(w.contains("1280x720") && w.contains("1920x1080"), "{w}");
        assert!(w.contains("This browser's camera"), "{w}");
    }
}
