//! Whether this machine runs shaders on the GPU, and a shader run to the end
//! on the calling thread for the preview strip.

use super::super::frame::Pic;
use super::gl::Gl;
use crate::overlay::blend::Planes;
use std::time::{Duration, Instant};

/// Whether GStreamer GL runs a shader on this machine: one frame through a
/// trivial transition. Asked once and remembered, and only when something
/// wants to know (`fx.list`, or the first shader take).
///
/// A GPU that is there but never answers (a runner with no GL context, where
/// `glupload` builds and then waits for good) counts as none: the answer
/// must come back inside three seconds. So does one that answers with the
/// wrong picture: half way from grey 128 to black 16 must read about 72. On
/// a macOS runner GL answered every frame with the old picture alone, so a
/// shader take showed no transition at all while the probe said yes.
pub fn available() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        // Asked from an import as well as from a running core, so it may be
        // the first thing in the process to touch GStreamer.
        if gstreamer::init().is_err() {
            return false;
        }
        let Ok(fragment) = super::fragment("vec4 transition(vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }") else { return false };
        let Ok(gl) = Gl::start(&fragment, (64, 36)) else { return false };
        let pic = vec![128u8; 64 * 36];
        let (mut y, mut u, mut v) = (vec![16u8; 64 * 36], vec![128u8; 32 * 18], vec![128u8; 32 * 18]);
        let old = Pic { y: &pic, u: &pic, v: &pic, strides: [64, 32, 32] };
        let mut planes = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [64, 32, 32], width: 64, height: 36 };
        let answered = settle(&gl, &old, &mut planes, 0.5, Duration::from_secs(3));
        gl.close();
        let mean = planes.y.iter().map(|&p| u32::from(p)).sum::<u32>() / planes.y.len() as u32;
        let mixed = (50..=100).contains(&mean);
        if answered && !mixed {
            tracing::warn!(mean, "GStreamer GL answered with the wrong picture (half way should read about 72); shaders run the software way");
        }
        answered && mixed
    })
}

/// Send one frame and draw the answer to it, waiting for it on this thread
/// up to `wait`. For the probe and the preview strip, never the programme.
pub fn settle(gl: &Gl, old: &Pic<'_>, planes: &mut Planes<'_>, t: f64, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    gl.forget();
    gl.mix(old, planes, t, None);
    while Instant::now() < deadline && !gl.has_failed() {
        std::thread::sleep(Duration::from_millis(15));
        if gl.answered_now() {
            // One frame went up, so this answer is its answer: draw it and
            // send nothing more, or the next call would meet this one's echo.
            gl.draw_latest(planes);
            return true;
        }
    }
    false
}
