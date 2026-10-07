//! Whether this machine runs shaders on the GPU, and a shader run to the end
//! on the calling thread for the preview strip.

use super::super::frame::Pic;
use super::gl::Gl;
use crate::overlay::blend::Planes;
use std::time::{Duration, Instant};

/// Whether GStreamer GL runs a shader on this machine: frames through a
/// trivial transition. Asked once and remembered, and only when something
/// wants to know (`fx.list`, or the first shader take).
///
/// A GPU that is there but never answers (a runner with no GL context, where
/// `glupload` builds and then waits for good) counts as none: each answer
/// must come back inside three seconds. So does one that answers with the
/// wrong picture. On a macOS runner a shader take on GL showed the old scene
/// for the whole window while one answer at progress 0.5 was right, so the
/// probe now sends two progress values on the same pipeline, as a take does,
/// and both must read as asked: from grey 128 to black 16, about 106 at 0.2
/// and about 38 at 0.8.
pub fn available() -> bool {
    static ANSWER: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ANSWER.get_or_init(|| {
        // Asked from an import as well as from a running core, so it may be
        // the first thing in the process to touch GStreamer.
        if gstreamer::init().is_err() {
            return false;
        }
        // Not on macOS for now. On the macOS runners this probe passed, and
        // so did each shader asked at the take's size (`moves`), and a take
        // still showed the old scene for its whole window, every run: the
        // GPU path there draws nothing while frames arrive one after another.
        // Until that is found on a Mac, the software versions and the
        // dissolve are what a take there shows, which at least moves.
        if cfg!(target_os = "macos") {
            tracing::info!("shaders run the software way on macOS: GStreamer GL there showed the old scene through a whole take");
            return false;
        }
        let Ok(fragment) = super::fragment("vec4 transition(vec2 uv) { return mix(getFromColor(uv), getToColor(uv), progress); }") else { return false };
        let Ok(gl) = Gl::start(&fragment, (64, 36)) else { return false };
        let read = [(0.2, 85..=125), (0.8, 20..=60)].into_iter().map(|(t, want)| (want, at(&gl, t))).collect::<Vec<_>>();
        gl.close();
        let right = read.iter().all(|(want, got)| got.is_some_and(|m| want.contains(&m)));
        if read.iter().all(|(_, got)| got.is_some()) && !right {
            let got: Vec<Option<u32>> = read.iter().map(|(_, g)| *g).collect();
            tracing::warn!(?got, "GStreamer GL answered with the wrong picture (about 106 at 0.2 and 38 at 0.8); shaders run the software way");
        }
        right
    })
}

/// The mean luma GL answers at progress `t`, grey 128 going to black 16.
fn at(gl: &Gl, t: f64) -> Option<u32> {
    let pic = vec![128u8; 64 * 36];
    let (mut y, mut u, mut v) = (vec![16u8; 64 * 36], vec![128u8; 32 * 18], vec![128u8; 32 * 18]);
    let old = Pic { y: &pic, u: &pic, v: &pic, strides: [64, 32, 32] };
    let mut planes = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [64, 32, 32], width: 64, height: 36 };
    if !settle(gl, &old, &mut planes, t, Duration::from_secs(3)) {
        return None;
    }
    Some(planes.y.iter().map(|&p| u32::from(p)).sum::<u32>() / planes.y.len() as u32)
}

/// Whether this shader, on this GPU, at the take's own size, draws
/// something other than the old picture half way through. The probe's
/// trivial mix passed on a macOS runner while a take of `glitch-slice` there
/// showed the old scene for its whole window, so each shader is asked once,
/// on its worker thread, before a take uses it. The old picture is grey 40
/// and the new a ramp from 120 to 240; at 0.5 any transition shows some of
/// the new.
pub fn moves(gl: &Gl, size: (i32, i32)) -> bool {
    let (w, h) = (size.0.max(2) as usize, size.1.max(2) as usize);
    let (y0, c) = (vec![40u8; w * h], vec![128u8; (w / 2) * (h / 2)]);
    let old = Pic { y: &y0, u: &c, v: &c, strides: [w, w / 2, w / 2] };
    let mut y: Vec<u8> = (0..w * h).map(|i| 120 + ((i % w) * 120 / w) as u8).collect();
    let (mut u, mut v) = (c.clone(), c.clone());
    let mut planes = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [w, w / 2, w / 2], width: w as i32, height: h as i32 };
    let answered = settle(gl, &old, &mut planes, 0.5, Duration::from_secs(3));
    gl.forget();
    let mean = planes.y.iter().map(|&p| u64::from(p)).sum::<u64>() / planes.y.len() as u64;
    answered && mean > 50
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
