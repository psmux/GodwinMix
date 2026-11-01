//! The look in a real pipeline: canvas video in, the bin, an appsink.

use super::*;
use crate::lut::{cube, Lut};
use gstreamer_app as gst_app;

fn canvas() -> CanvasCaps {
    CanvasCaps { width: 64, height: 36, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 }
}

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("gmx-look-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The first frame's Y, U and V at the top left of a red test picture.
fn first_pixel(look: &mut Look, params: &str) -> (u8, u8, u8) {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("videotestsrc").property("num-buffers", 1).property_from_str("pattern", "red").build().unwrap();
    let caps = gst::ElementFactory::make("capsfilter").property("caps", canvas().video()).build().unwrap();
    let bin = look.build(&canvas(), &toml::from_str(params).unwrap()).unwrap();
    let sink = gst_app::AppSink::builder().sync(false).build();
    pipeline.add_many([&src, &caps, &bin, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([&src, &caps, &bin, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let sample = sink.pull_sample().expect("a frame");
    let info = gst_video::VideoInfo::from_caps(sample.caps().unwrap()).unwrap();
    let frame = gst_video::VideoFrameRef::from_buffer_ref_readable(sample.buffer().unwrap(), &info).unwrap();
    let px = (frame.plane_data(0).unwrap()[0], frame.plane_data(1).unwrap()[0], frame.plane_data(2).unwrap()[0]);
    drop(frame);
    pipeline.set_state(gst::State::Null).unwrap();
    px
}

fn grey((_, u, v): (u8, u8, u8)) -> bool {
    (u as i32 - 128).abs() <= 2 && (v as i32 - 128).abs() <= 2
}

#[test]
fn nothing_set_leaves_red_red() {
    let px = first_pixel(&mut Look::in_dir(scratch("none")), "");
    assert!(!grey(px), "{px:?}");
}

#[test]
fn a_built_in_look_by_name_runs_on_the_frame() {
    let px = first_pixel(&mut Look::in_dir(scratch("named")), "lut = \"black-and-white\"");
    assert!(grey(px), "{px:?}");
}

#[test]
fn an_amount_of_zero_is_no_look() {
    let dir = scratch("zero");
    let plain = first_pixel(&mut Look::in_dir(dir.clone()), "");
    assert_eq!(first_pixel(&mut Look::in_dir(dir), "lut = \"black-and-white\"\namount = 0.0"), plain);
}

#[test]
fn a_file_is_read_directly() {
    let dir = scratch("file");
    let path = dir.join("mono.cube");
    let mono = Lut::from_fn("mono", 9, |[r, g, b]| [0.2126 * r + 0.7152 * g + 0.0722 * b; 3]);
    std::fs::write(&path, cube::write(&mono)).unwrap();
    let px = first_pixel(&mut Look::in_dir(dir), &format!("file = {:?}", path.display().to_string()));
    assert!(grey(px), "{px:?}");
}

#[test]
fn a_preset_fills_in_the_lut() {
    let dir = scratch("preset");
    library::save_preset(&dir, library::Preset { name: "news".into(), lut: Some("black-and-white".into()), ..Default::default() }).unwrap();
    let px = first_pixel(&mut Look::in_dir(dir), "preset = \"news\"");
    assert!(grey(px), "{px:?}");
}

#[test]
fn a_missing_lut_leaves_the_picture_and_a_change_to_it_is_refused() {
    let dir = scratch("missing");
    let mut look = Look::in_dir(dir);
    let px = first_pixel(&mut look, "lut = \"not-there\"");
    assert!(!grey(px), "{px:?}");
    let err = look.configure(&toml::from_str("lut = \"not-there\"").unwrap()).unwrap_err().to_string();
    assert!(err.contains("lut.import"), "{err}");
}

#[test]
fn a_change_on_air_bakes_and_lands() {
    let dir = scratch("onair");
    let look = Look::in_dir(dir);
    look.set_later(&toml::from_str("lut = \"sepia\"").unwrap()).unwrap();
    for _ in 0..100 {
        if look.state.lock().table.is_some() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("the bake never landed");
}
