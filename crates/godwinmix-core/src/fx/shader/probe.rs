//! Whether this machine runs shaders on the GPU, and a shader run to the end
//! on the calling thread for the preview strip.

use super::super::frame::Pic;
use super::gl::Gl;
use crate::overlay::blend::Planes;
use std::time::{Duration, Instant};

/// Whether GStreamer GL runs a shader on this machine: one frame through a
/// trivial transition. Asked once and remembered, and only when something
/// wants to know (`fx.list`, or the first shader take).
pub fn available() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        let Ok(fragment) = super::fragment("vec4 transition(vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }") else { return false };
        let Ok(gl) = Gl::start(&fragment, (64, 36)) else { return false };
        let pic = vec![128u8; 64 * 36];
        let (mut y, mut u, mut v) = (vec![16u8; 64 * 36], vec![128u8; 32 * 18], vec![128u8; 32 * 18]);
        let old = Pic { y: &pic, u: &pic, v: &pic, strides: [64, 32, 32] };
        let mut planes = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [64, 32, 32], width: 64, height: 36 };
        let answered = settle(&gl, &old, &mut planes, 0.5, Duration::from_secs(3));
        gl.close();
        answered
    })
}

/// Send one frame and draw the answer to it, waiting for it on this thread
/// up to `wait`. For the probe and the preview strip, never the programme.
pub fn settle(gl: &Gl, old: &Pic<'_>, planes: &mut Planes<'_>, t: f64, wait: Duration) -> bool {
    let deadline = Instant::now() + wait;
    gl.forget();
    gl.mix(old, planes, t);
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
