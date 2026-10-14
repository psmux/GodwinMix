//! On Windows nothing the fx code does loads the GL plugin, and with it the
//! GL driver that crashed the show beside the hardware video decoder. A file
//! of its own so that it runs in a process no other test has put GL into.
//! See `fx::shader::gate`.

use godwinmix_core::fx::frame::Pic;
use godwinmix_core::fx::shader::{gate, gl::Gl, probe, runs, ShaderMix};
use godwinmix_core::fx::Mix;
use godwinmix_core::overlay::blend::Planes;
use gstreamer as gst;

const W: usize = 64;
const H: usize = 36;

fn gl_loaded() -> bool {
    gst::Registry::get().find_plugin("opengl").is_some_and(|p| p.is_loaded())
}

#[test]
fn on_windows_a_shader_take_and_the_probe_leave_gl_unloaded() {
    gst::init().unwrap();
    if std::env::var(gate::SWITCH_ENV).is_ok() || !cfg!(windows) {
        println!("skipped: GL is allowed here, by platform or by {}", gate::SWITCH_ENV);
        return;
    }
    assert!(!gate::allowed());
    assert!(!probe::available(), "the probe says no GPU without starting GL");
    assert_eq!(runs("ripple"), "cpu", "ripple runs its software version");
    assert_eq!(runs("an-imported-shader"), "fade", "a shader with no software version dissolves");
    let err = Gl::start("void main() {}", (W as i32, H as i32)).err().expect("GL refuses to start");
    assert!(err.to_string().contains(gate::SWITCH_ENV), "the refusal names the switch: {err}");

    // A take's whole window, the way the programme drives it.
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../graphics/starters/ripple/ripple.glsl")).unwrap();
    let mix = ShaderMix::start("ripple", &src, (W as i32, H as i32));
    let old = [vec![40u8; W * H], vec![128u8; W * H / 4], vec![128u8; W * H / 4]];
    for i in 0..=10 {
        let [mut y, mut u, mut v] = [vec![200u8; W * H], vec![128u8; W * H / 4], vec![128u8; W * H / 4]];
        let mut f = Planes { y: &mut y, u: &mut u, v: &mut v, strides: [W, W / 2, W / 2], width: W as i32, height: H as i32 };
        mix.mix(&Pic { y: &old[0], u: &old[1], v: &old[2], strides: [W, W / 2, W / 2] }, &mut f, i as f64 / 10.0);
        if i == 10 {
            assert!(y.iter().all(|&p| p == 200), "the software ripple ends on the new picture");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(!gl_loaded(), "the GL plugin was loaded, and with it the GL driver");
}
