//! Shader transitions: the GPU path through GStreamer GL where it runs, and
//! the software versions of the shipped shaders, each checked at both ends
//! (all old at 0, all new at 1) on real frames.

use godwinmix_core::fx::frame::Pic;
use godwinmix_core::fx::shader::{cpu, fragment, gl::Gl, probe, runs, ShaderMix};
use godwinmix_core::fx::Mix;
use godwinmix_core::overlay::blend::Planes;
use gstreamer as gst;
use std::time::{Duration, Instant};

const W: usize = 64;
const H: usize = 36;

fn frame(y: u8, u: u8, v: u8) -> [Vec<u8>; 3] {
    [vec![y; W * H], vec![u; W * H / 4], vec![v; W * H / 4]]
}

fn shader(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("../../graphics/starters/{name}/{name}.glsl"));
    std::fs::read_to_string(path).unwrap()
}

/// Run one step of a mix and give back the Y plane.
fn step(mix: &dyn Fn(&Pic<'_>, &mut Planes<'_>), t_new: [Vec<u8>; 3]) -> Vec<u8> {
    let old = frame(40, 160, 100);
    let mut f = t_new;
    let [y, u, v] = &mut f;
    let mut planes = Planes { y, u, v, strides: [W, W / 2, W / 2], width: W as i32, height: H as i32 };
    mix(&Pic { y: &old[0], u: &old[1], v: &old[2], strides: [W, W / 2, W / 2] }, &mut planes);
    f[0].clone()
}

#[test]
fn the_shipped_shaders_have_software_versions_that_start_old_and_end_new() {
    for name in ["glitch-slice", "ripple"] {
        let run = cpu::find(name).expect("a software version");
        let at0 = step(&|o, f| run(o, f, 0.0), frame(200, 90, 200));
        let at1 = step(&|o, f| run(o, f, 1.0), frame(200, 90, 200));
        assert!(at0.iter().all(|&y| y == 40), "{name} at 0 is the old picture");
        assert!(at1.iter().all(|&y| y == 200), "{name} at 1 is the new picture");
    }
}

/// Where GStreamer GL builds but no GL context answers (a hosted Linux runner
/// with no GPU, the Windows runner), the probe says so and a shader take
/// runs the software version: that is what is checked there, and the GPU half
/// is skipped with the reason printed.
#[test]
fn a_shader_runs_on_the_gpu_where_gstreamer_gl_does() {
    gst::init().unwrap();
    if !probe::available() {
        println!("skipped the GPU half: GStreamer GL gave no answer here in three seconds, so takes run the software version");
        assert_eq!(runs("ripple"), "cpu", "with no GPU a shader that has a software version runs it");
        let mix = ShaderMix::start("ripple", &shader("ripple"), (W as i32, H as i32));
        let at1 = step(&|o, f| mix.mix(o, f, 1.0), frame(200, 128, 128));
        assert!(at1.iter().all(|&y| y == 200), "the software ripple at 1 is the new picture");
        return;
    }
    let source = fragment(&shader("ripple")).expect("ripple is a gl-transitions shader");
    let gl = Gl::start(&source, (W as i32, H as i32)).expect("the probe ran GL, so the shader starts");
    assert!(probe::moves(&gl, (W as i32, H as i32)), "ripple half way through on this GPU is the old picture alone");
    // The answer comes back a frame later; send until one has.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut last = Vec::new();
    for t in [1.0, 1.0] {
        while Instant::now() < deadline {
            // Grey, so the trip through RGB on the GPU loses nothing to
            // clipping and the number read back is the number sent.
            last = step(&|o, f| gl.mix(o, f, t, None), frame(200, 128, 128));
            if gl.answered() {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        std::thread::sleep(Duration::from_millis(60));
    }
    assert!(gl.answered(), "the probe had an answer from GStreamer GL but this shader gave none in five seconds");
    let mean = last.iter().map(|&y| y as u32).sum::<u32>() / last.len() as u32;
    println!("GPU ripple at 1: mean luma {mean} (new is 200, old 40)");
    assert!(mean > 190, "at progress 1 the GPU draws the new scene, mean luma was {mean}");
}
