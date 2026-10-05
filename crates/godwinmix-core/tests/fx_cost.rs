//! What each fx costs a 1080p programme frame on this machine, on one core,
//! and how fast a starter clip decodes on its own pipeline.
//!
//! ```bash
//! cargo test -p godwinmix-core --release --test fx_cost -- --ignored --nocapture
//! ```
//!
//! Prints one line per mode: milliseconds per frame and the share of a
//! 33.3 ms frame at 30 fps. Run on purpose, never in CI: a number from a
//! shared runner says nothing.

use godwinmix_core::fx::frame::Pic;
use godwinmix_core::fx::matte::{dissolve, Matte};
use godwinmix_core::fx::player::Player;
use godwinmix_core::fx::shader::cpu;
use godwinmix_core::fx::Mix;
use godwinmix_core::overlay::blend::{Draw, Planes, Rect, Source};
use godwinmix_core::overlay::modes::{self, Mode};
use gstreamer as gst;
use std::time::{Duration, Instant};

const W: usize = 1920;
const H: usize = 1080;
const RUNS: u32 = 60;

fn frame(y: u8) -> [Vec<u8>; 3] {
    [vec![y; W * H], vec![120; W * H / 4], vec![140; W * H / 4]]
}

/// An AYUV light leak stand in: a soft warm ramp left to right, black on
/// the right third, which is what a leak or a burn looks like half way.
fn leak(w: usize, h: usize) -> Vec<u8> {
    let mut px = vec![0u8; w * h * 4];
    for y in 0..h {
        for x in 0..w {
            let v = (255 - (x * 255 / (w * 2 / 3)).min(255)) as u8;
            px[(y * w + x) * 4..][..4].copy_from_slice(&[255, 16 + v * 219 / 255, 110, 160]);
        }
    }
    px
}

fn time(name: &str, mut f: impl FnMut(&mut Planes<'_>, f64)) {
    let mut fr = frame(90);
    let started = Instant::now();
    for i in 0..RUNS {
        let [y, u, v] = &mut fr;
        let mut p = Planes { y, u, v, strides: [W, W / 2, W / 2], width: W as i32, height: H as i32 };
        f(&mut p, i as f64 / RUNS as f64);
    }
    let ms = started.elapsed().as_secs_f64() * 1000.0 / RUNS as f64;
    println!("{name:<22} {ms:>6.2} ms a frame, {:>4.0} percent of a 30 fps frame", ms / 33.33 * 100.0);
}

#[test]
#[ignore]
fn what_each_fx_costs_a_1080p_frame() {
    // A stinger is drawn at the canvas size, light at half of it, as the
    // player decodes them (`fx::decode_size`).
    let (full, half) = (leak(W, H), leak(W / 2, H / 2));
    let whole = Rect::new(0, 0, W as i32, H as i32);
    let draw = Draw { window: whole, to: whole, clip: whole, alpha: 255 };
    let small = Draw { window: Rect::new(0, 0, W as i32 / 2, H as i32 / 2), ..draw };
    time("normal (alpha), full", |p, _| modes::draw(p, &Source { data: &full, stride: W * 4 }, &draw, Mode::Normal));
    for (name, mode) in [("screen, half", Mode::Screen), ("add, half", Mode::Add), ("luma key, half", Mode::Luma)] {
        time(name, |p, _| modes::draw(p, &Source { data: &half, stride: W * 2 }, &small, mode));
    }
    let clear = vec![0u8; W * H * 4];
    time("clear (loop alone)", |p, _| modes::draw(p, &Source { data: &clear, stride: W * 4 }, &draw, Mode::Normal));
    let old = frame(40);
    let pic = Pic { y: &old[0], u: &old[1], v: &old[2], strides: [W, W / 2, W / 2] };
    let ramp: Vec<u8> = (0..H).flat_map(|_| (0..W).map(|x| (x * 255 / W) as u8)).collect();
    let matte = Matte::from_plane(ramp, W, 0.1, false);
    time("luma matte", |p, t| matte.mix(&pic, p, t));
    time("dissolve", |p, t| dissolve(&pic, p, t));
    for name in ["glitch-slice", "ripple"] {
        let s = cpu::find(name).unwrap();
        time(&format!("{name} (software)"), |p, t| s(&pic, p, t.max(0.3)));
    }
}

#[test]
#[ignore]
fn how_fast_a_starter_clip_decodes_at_1080p() {
    gst::init().unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../graphics/starters");
    for (name, file) in [("light-leak", "light-leak.webm"), ("glitch", "glitch.webm")] {
        let player = Player::start(&root.join(name).join(file), (W as i32, H as i32)).unwrap();
        let started = Instant::now();
        let mut frames = 0;
        let (mut first, mut last) = (None, Duration::ZERO);
        while started.elapsed() < Duration::from_secs(10) {
            let wait = if frames == 0 { 5_000 } else { 500 };
            match player.sink.try_pull_sample(gst::ClockTime::from_mseconds(wait)) {
                Some(_) => {
                    frames += 1;
                    first.get_or_insert(started.elapsed());
                    last = started.elapsed();
                }
                None => break,
            }
        }
        let secs = (last - first.unwrap_or_default()).as_secs_f64().max(0.001);
        println!("{name:<12} first frame {:>5.0} ms, {frames} frames at 1080p AYUV in {secs:.2} s: {:.0} frames a second", first.unwrap_or_default().as_secs_f64() * 1000.0, frames as f64 / secs);
        assert!(frames > 0, "{name} gave no frames; failed: {}", player.shared.failed.load(std::sync::atomic::Ordering::Acquire));
        player.stop();
    }
}
