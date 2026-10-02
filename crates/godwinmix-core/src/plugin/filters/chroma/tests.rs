use super::colour::rgb_to_yuv;
use super::frame::{self, Region, Scratch, I420};
use super::lut::Lut;
use super::params::{Colour, Matte, Settings};
use super::sample;
use crate::config::Params;

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

fn keyed(data: &[u8], w: usize, h: usize, s: &Settings) -> (Region, Vec<u8>) {
    let key = match s.colour {
        Colour::Rgb(rgb) => rgb_to_yuv(rgb),
        Colour::Auto => unreachable!("these tests give the colour"),
    };
    let lut = Lut::build((key.1, key.2), s);
    let r = Region::of(w, h, &s.matte);
    let mut out = vec![0u8; r.w * r.h * 4];
    frame::key(&view(data, w, h), r, &lut, s.feather, &mut Scratch::default(), &mut out);
    (r, out)
}

fn green_key() -> Settings {
    Settings { colour: Colour::Rgb([0, 255, 0]), ..Settings::default() }
}

#[test]
fn a_pure_green_pixel_keys_to_fully_clear_and_skin_stays_solid() {
    let (w, h) = (64, 64);
    let data = frame(w, h, [0, 255, 0], Some(([224, 172, 140], 16, 16, 32)));
    let (_, out) = keyed(&data, w, h, &green_key());
    let alpha = |x: usize, y: usize| out[(y * w + x) * 4];
    assert_eq!(alpha(2, 2), 0, "the green corner is clear");
    assert_eq!(alpha(60, 40), 0, "and so is the other side");
    assert_eq!(alpha(32, 32), 255, "the middle of the skin square is solid");
    let skin = rgb_to_yuv([224, 172, 140]);
    assert_eq!(&out[(32 * w + 32) * 4 + 1..(32 * w + 32) * 4 + 4], &[skin.0, skin.1, skin.2], "and keeps its colour");
}

#[test]
fn spill_suppression_takes_the_green_out_of_an_edge_pixel() {
    // Grey lit by a green screen: what the edge of a white shirt looks like.
    let (w, h) = (16, 16);
    let edge = [150, 185, 150];
    let data = frame(w, h, edge, None);
    let (_, plain) = keyed(&data, w, h, &Settings { spill: 0.0, ..green_key() });
    let (_, clean) = keyed(&data, w, h, &Settings { spill: 1.0, ..green_key() });
    let green = |px: &[u8]| (128 - px[2] as i32) + (128 - px[3] as i32);
    assert_eq!(plain[0], 255, "a grey that leans green is still solid");
    assert!(green(&plain[..4]) > 6, "with no suppression the green stays: {:?}", &plain[..4]);
    assert!(green(&clean[..4]) <= 1, "with suppression it is gone: {:?}", &clean[..4]);
}

#[test]
fn the_garbage_matte_removes_everything_outside_it() {
    let (w, h) = (100, 60);
    // A white frame: nothing in it is green, so only the matte can clear it.
    let data = frame(w, h, [255, 255, 255], None);
    let matte = Matte { left: 0.2, right: 0.3, top: 0.1, bottom: 0.0 };
    let (r, out) = keyed(&data, w, h, &Settings { matte, ..green_key() });
    assert_eq!(r, Region { x: 20, y: 6, w: 50, h: 54 }, "only the kept area is in the picture");
    assert!(out.chunks(4).all(|px| px[0] == 255), "everything inside the matte is solid");
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
    let shot = sample::studio(640, 360);
    let under = [200, 40, 160];
    let g = super::guess::dominant(
        view(&shot.i420, 640, 360).u.iter().zip(view(&shot.i420, 640, 360).v).map(|(u, v)| (100, *u, *v)),
        super::params::Family::Green,
    )
    .expect("the screen is found");
    let s = Settings { colour: Colour::Rgb(g.rgb), ..Settings::default() };
    let (_, new) = keyed(&shot.i420, 640, 360, &s);
    let (new_err, new_fringe) = sample::score(&shot, &new, under);
    let Some(old) = old_key(&shot) else {
        println!("skipping the comparison: this GStreamer has no `alpha` element");
        return;
    };
    let (old_err, old_fringe) = sample::score(&shot, &old, under);
    println!("luma error new {new_err:.2} old {old_err:.2}; green fringe new {new_fringe:.2} old {old_fringe:.2}");
    assert!(new_err < old_err, "the new key lands nearer the true composite");
    assert!(new_fringe < old_fringe, "and leaves less green on the edges");
}

/// What GStreamer's `alpha` element, the old key, makes of the same shot,
/// at its defaults with the screen colour given.
fn old_key(shot: &sample::Shot) -> Option<Vec<u8>> {
    use gstreamer as gst;
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
        let lut = Lut::build((rgb_to_yuv(sample::SCREEN).1, rgb_to_yuv(sample::SCREEN).2), &s);
        let mut scratch = Scratch::default();
        let r = Region::of(w, h, &s.matte);
        let mut canvas = vec![16u8; w * h * 3 / 2];
        let n = 200;
        let (mut keying, mut drawing) = (0f64, 0f64);
        for _ in 0..n {
            let t = std::time::Instant::now();
            let mut out = vec![0u8; r.w * r.h * 4];
            frame::key(&view(&shot.i420, w, h), r, &lut, s.feather, &mut scratch, &mut out);
            keying += t.elapsed().as_secs_f64();
            let t = std::time::Instant::now();
            draw(&mut canvas, w, h, &out);
            drawing += t.elapsed().as_secs_f64();
        }
        println!(
            "{w}x{h}: key {:.2} ms, board draw {:.2} ms a frame; at 30 fps {:.1} percent of one core",
            keying * 1000.0 / n as f64,
            drawing * 1000.0 / n as f64,
            (keying + drawing) / n as f64 * 30.0 * 100.0
        );
    }
}

/// The board's own blend, onto an I420 canvas the size of the picture.
fn draw(canvas: &mut [u8], w: usize, h: usize, ayuv: &[u8]) {
    use crate::overlay::blend::{self, Draw, Planes, Rect, Source};
    let (y, rest) = canvas.split_at_mut(w * h);
    let (u, v) = rest.split_at_mut(w * h / 4);
    let mut planes = Planes { y, u, v, strides: [w, w / 2, w / 2], width: w as i32, height: h as i32 };
    let all = Rect::new(0, 0, w as i32, h as i32);
    blend::draw(&mut planes, &Source { data: ayuv, stride: w * 4 }, &Draw { window: all, to: all, clip: all, alpha: 255 });
}
