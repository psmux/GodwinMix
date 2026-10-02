//! The key, from a frame to the pixels the board draws, on frames whose
//! answer is known.

use super::colour::rgb_to_yuv;
use super::frame::{self, Region, I420};
use super::lut::Lut;
use super::params::{Colour, Matte, Settings};
use super::sample;
use crate::config::Params;
use crate::overlay::blend::{self, Draw, Planes, Rect, Source};
use crate::overlay::keyed::{self, Keyed};
use gstreamer as gst;
use gstreamer_video as gst_video;

/// A flat I420 frame of one colour, with an optional square of another.
fn frame(w: usize, h: usize, rgb: [u8; 3], square: Option<([u8; 3], usize, usize, usize)>) -> Vec<u8> {
    let mut px = vec![rgb; w * h];
    if let Some((c, x0, y0, side)) = square {
        for y in y0..y0 + side {
            for x in x0..x0 + side {
                px[y * w + x] = c;
            }
        }
    }
    sample::to_i420(&px, w, h)
}

fn view(data: &[u8], w: usize, h: usize) -> I420<'_> {
    let (y, rest) = data.split_at(w * h);
    let (u, v) = rest.split_at(w * h / 4);
    I420 { y, u, v, strides: [w, w / 2, w / 2], width: w, height: h }
}

fn planes(canvas: &mut [u8], w: usize, h: usize) -> Planes<'_> {
    let (y, rest) = canvas.split_at_mut(w * h);
    let (u, v) = rest.split_at_mut(w * h / 4);
    Planes { y, u, v, strides: [w, w / 2, w / 2], width: w as i32, height: h as i32 }
}

/// The key's decision for a frame, as the filter hands it to the board.
/// Widths here are multiples of 8, so the packed frame and GStreamer's own
/// strides agree.
fn decide(data: &[u8], w: usize, h: usize, s: &Settings) -> Keyed {
    let _ = gst::init();
    let Colour::Rgb(rgb) = s.colour else { unreachable!("these tests give the colour") };
    let (_, ku, kv) = rgb_to_yuv(rgb);
    let r = Region::of(w, h, &s.matte);
    let (mut alpha, mut chroma) = (Vec::new(), Vec::new());
    frame::blocks(&view(data, w, h), r, &Lut::build((ku, kv), s), s.feather, &mut alpha, &mut chroma);
    let info = gst_video::VideoInfo::builder(gst_video::VideoFormat::I420, w as u32, h as u32).build().unwrap();
    Keyed { frame: gst::Buffer::from_slice(data.to_vec()), info, region: (r.x, r.y, r.w, r.h), alpha, chroma }
}

/// Key a frame and draw it over a canvas of `under` the way the board does.
fn composite(data: &[u8], w: usize, h: usize, s: &Settings, under: [u8; 3]) -> Vec<u8> {
    let k = decide(data, w, h, s);
    let mut canvas = sample::to_i420(&vec![under; w * h], w, h);
    let (rx, ry, rw, rh) = k.region;
    let d = Draw {
        window: Rect::new(0, 0, rw as i32, rh as i32),
        to: Rect::new(rx as i32, ry as i32, rw as i32, rh as i32),
        clip: Rect::new(0, 0, w as i32, h as i32),
        alpha: 255,
    };
    keyed::draw(&mut planes(&mut canvas, w, h), &k, &d);
    canvas
}

fn yuv(canvas: &[u8], w: usize, h: usize, x: usize, y: usize) -> (u8, u8, u8) {
    let c = (y / 2) * (w / 2) + x / 2;
    (canvas[y * w + x], canvas[w * h + c], canvas[w * h + w * h / 4 + c])
}

const UNDER: [u8; 3] = [200, 40, 160];

fn green_key() -> Settings {
    Settings { colour: Colour::Rgb([0, 255, 0]), ..Settings::default() }
}

#[test]
fn a_pure_green_pixel_keys_to_fully_clear_and_skin_stays_solid() {
    let (w, h) = (64, 64);
    let data = frame(w, h, [0, 255, 0], Some(([224, 172, 140], 16, 16, 32)));
    let out = composite(&data, w, h, &green_key(), UNDER);
    assert_eq!(yuv(&out, w, h, 2, 2), rgb_to_yuv(UNDER), "the green corner shows what is under it");
    assert_eq!(yuv(&out, w, h, 60, 40), rgb_to_yuv(UNDER), "and so does the other side");
    assert_eq!(yuv(&out, w, h, 32, 32), rgb_to_yuv([224, 172, 140]), "the skin is solid and keeps its colour");
}

#[test]
fn spill_suppression_takes_the_green_out_of_an_edge_pixel() {
    // Grey lit by a green screen: what the edge of a white shirt looks like.
    let (w, h) = (16, 16);
    let data = frame(w, h, [150, 185, 150], None);
    let plain = yuv(&composite(&data, w, h, &Settings { spill: 0.0, ..green_key() }, UNDER), w, h, 8, 8);
    let clean = yuv(&composite(&data, w, h, &Settings { spill: 1.0, ..green_key() }, UNDER), w, h, 8, 8);
    let green = |p: (u8, u8, u8)| (128 - p.1 as i32) + (128 - p.2 as i32);
    assert_eq!(plain.0, rgb_to_yuv([150, 185, 150]).0, "a grey that leans green is still solid");
    assert!(green(plain) > 6, "with no suppression the green stays: {plain:?}");
    assert!(green(clean) <= 1, "with suppression it is gone: {clean:?}");
}

#[test]
fn the_garbage_matte_removes_everything_outside_it() {
    let (w, h) = (96, 64);
    // A white frame: nothing in it is green, so only the matte can clear it.
    let data = frame(w, h, [255, 255, 255], None);
    let matte = Matte { left: 0.25, right: 0.25, top: 0.125, bottom: 0.0 };
    let s = Settings { matte, ..green_key() };
    assert_eq!(decide(&data, w, h, &s).region, (24, 8, 48, 56), "only the kept area is decided");
    let out = composite(&data, w, h, &s, UNDER);
    assert_eq!(yuv(&out, w, h, 10, 30), rgb_to_yuv(UNDER), "left of the matte is gone");
    assert_eq!(yuv(&out, w, h, 40, 2), rgb_to_yuv(UNDER), "and above it");
    assert_eq!(yuv(&out, w, h, 40, 30), rgb_to_yuv([255, 255, 255]), "inside it the picture is solid");
}

#[test]
fn settings_read_obs_numbers_and_name_a_bad_field() {
    let params: Params = toml::from_str("similarity = 400\nsmoothness = 80\nkey_color_type = \"green\"").unwrap();
    let s = Settings::from_params(&params).unwrap();
    assert!((s.similarity - 0.4).abs() < 1e-6 && (s.smoothness - 0.08).abs() < 1e-6);
    let bad: Params = toml::from_str("spill = 3000").unwrap();
    let e = format!("{}", Settings::from_params(&bad).unwrap_err());
    assert!(e.contains("params.spill") && e.contains("0 to 1"), "{e}");
    let unknown: Params = toml::from_str("colour_key = 1").unwrap();
    let e = format!("{}", Settings::from_params(&unknown).unwrap_err());
    assert!(e.contains("similarity") && e.contains("matte_left"), "{e}");
    let empty: Params = toml::from_str("matte_left = 0.6\nmatte_right = 0.5").unwrap();
    assert!(Settings::from_params(&empty).is_err(), "a matte that keeps nothing is refused");
}

#[test]
fn the_new_key_beats_the_old_one_on_a_shot_with_hair_blur_and_spill() {
    let (w, h) = (640, 360);
    let shot = sample::studio(w, h);
    let v = view(&shot.i420, w, h);
    let g = super::guess::dominant(v.u.iter().zip(v.v).map(|(u, v)| (100, *u, *v)), super::params::Family::Green)
        .expect("the screen is found");
    let s = Settings { colour: Colour::Rgb(g.rgb), ..Settings::default() };
    let (new_err, new_fringe) = sample::score(&shot, &composite(&shot.i420, w, h, &s, UNDER), UNDER);
    let Some(old) = old_key(&shot) else {
        println!("skipping the comparison: this GStreamer has no `alpha` element");
        return;
    };
    // The old key's AYUV, drawn by the board's own blend for a fair match.
    let mut canvas = sample::to_i420(&vec![UNDER; w * h], w, h);
    let all = Rect::new(0, 0, w as i32, h as i32);
    let d = Draw { window: all, to: all, clip: all, alpha: 255 };
    blend::draw(&mut planes(&mut canvas, w, h), &Source { data: &old, stride: w * 4 }, &d);
    let (old_err, old_fringe) = sample::score(&shot, &canvas, UNDER);
    println!("luma error new {new_err:.2} old {old_err:.2}; green fringe new {new_fringe:.2} old {old_fringe:.2}");
    assert!(new_err < old_err, "the new key lands nearer the true composite");
    assert!(new_fringe < old_fringe, "and leaves less green on the edges");
}

/// What GStreamer's `alpha` element, the old key, makes of the same shot,
/// at its defaults with the screen colour given, as AYUV.
fn old_key(shot: &sample::Shot) -> Option<Vec<u8>> {
    use gstreamer::prelude::*;
    gst::init().ok()?;
    if !crate::probe::exists("alpha") {
        return None;
    }
    let (w, h) = (shot.width, shot.height);
    let [r, g, b] = sample::SCREEN;
    let pipe = gst::parse::launch(&format!(
        "appsrc name=s format=time caps=video/x-raw,format=I420,width={w},height={h},framerate=30/1,colorimetry=bt709 ! \
         videoconvert ! alpha method=custom target-r={r} target-g={g} target-b={b} ! videoconvert ! \
         video/x-raw,format=AYUV ! appsink name=k sync=false"
    ))
    .ok()?
    .downcast::<gst::Pipeline>()
    .ok()?;
    let src = pipe.by_name("s")?.downcast::<gstreamer_app::AppSrc>().ok()?;
    let sink = pipe.by_name("k")?.downcast::<gstreamer_app::AppSink>().ok()?;
    pipe.set_state(gst::State::Playing).ok()?;
    src.push_buffer(gst::Buffer::from_slice(shot.i420.clone())).ok()?;
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5));
    let _ = pipe.set_state(gst::State::Null);
    let map = sample?.buffer()?.map_readable().ok()?.to_vec();
    Some(map)
}

#[test]
#[ignore = "a measurement: cargo test -p godwinmix-core --release key_cost -- --ignored --nocapture"]
fn key_cost_per_frame() {
    for (w, h) in [(1280, 720), (1920, 1080)] {
        let shot = sample::studio(w, h);
        let s = Settings { colour: Colour::Rgb(sample::SCREEN), ..Settings::default() };
        let (_, ku, kv) = rgb_to_yuv(sample::SCREEN);
        let lut = Lut::build((ku, kv), &s);
        let mut canvas = vec![16u8; w * h * 3 / 2];
        let k0 = decide(&shot.i420, w, h, &s);
        let all = Rect::new(0, 0, w as i32, h as i32);
        let d = Draw { window: all, to: all, clip: all, alpha: 255 };
        let n = 200;
        let (mut deciding, mut drawing) = (0f64, 0f64);
        for _ in 0..n {
            let t = std::time::Instant::now();
            let (mut a, mut c) = (Vec::new(), Vec::new());
            frame::blocks(&view(&shot.i420, w, h), Region::of(w, h, &s.matte), &lut, s.feather, &mut a, &mut c);
            deciding += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            keyed::draw(&mut planes(&mut canvas, w, h), &k0, &d);
            drawing += t.elapsed().as_secs_f64();
        }
        println!(
            "{w}x{h}: key {:.2} ms, board draw {:.2} ms a frame; at 30 fps {:.1} percent of one core",
            deciding * 1000.0 / n as f64,
            drawing * 1000.0 / n as f64,
            (deciding + drawing) / n as f64 * 30.0 * 100.0
        );
    }
}
