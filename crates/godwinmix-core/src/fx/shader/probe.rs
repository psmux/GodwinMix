//! Whether this machine runs shaders on the GPU.

use super::super::frame::Pic;
use super::gl::Gl;
use crate::overlay::blend::Planes;

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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline {
            let mut planes = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [64, 32, 32], width: 64, height: 36 };
            gl.mix(&old, &mut planes, 0.5);
            if gl.answered() {
                return true;
            }
            if gl.has_failed() {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        false
    })
}
