//! The follow loop's detector: YOLOX once it has loaded, the motion
//! detector while it loads and when it cannot, and the setting that picks.

use std::time::{Duration, Instant};

use godwinmix_track::{Detector, FrameRef, PixelFormat};
use gmx_ptz::follow::detector::{Kind, Switching};
use gmx_ptz::Settings;
use serde_json::json;

fn photo() -> (u32, u32, Vec<u8>) {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../crates/godwinmix-track/tests/images/two-people.jpg");
    let img = image::open(path).expect("the committed photograph").to_rgb8();
    let img = image::imageops::resize(&img, 320, 240, image::imageops::FilterType::Triangle);
    (img.width(), img.height(), img.into_raw())
}

/// Detect until the loader has finished one way or the other.
fn settle(d: &mut Switching, frame: &FrameRef) -> usize {
    let until = Instant::now() + Duration::from_secs(120);
    let mut found = d.detect(frame).len();
    while d.status.stage == "loading" && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(50));
        found = d.detect(frame).len();
    }
    found
}

#[test]
fn the_setting_picks_the_detector_and_names_a_wrong_one() {
    assert_eq!(Settings::default().detector, Kind::Yolox);
    assert_eq!(Settings::from_value(&json!({"detector": "motion"})).detector, Kind::Motion);
    assert_eq!(Settings::from_value(&json!({"detector": "yolox+pose"})).detector, Kind::YoloxPose);
    let s = Settings::from_value(&json!({"detector": "hog"}));
    assert_eq!(s.detector, Kind::Yolox);
    assert!(s.problems[0].contains("motion, yolox, yolox+pose"), "{:?}", s.problems);
}

#[test]
fn motion_asked_for_is_motion_with_no_model() {
    let d = Switching::new(Kind::Motion, None);
    assert_eq!((d.status.running, d.status.stage), (Kind::Motion, "ready"));
    assert!(d.status.why.contains("asked for"));
}

#[test]
fn a_model_that_will_not_load_leaves_motion_running_and_says_why() {
    let dir = std::env::temp_dir().join(format!("gmx-ptz-bad-model-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("yolox_nano.onnx"), b"not a model").unwrap();
    let mut d = Switching::new(Kind::Yolox, Some(dir));
    assert_eq!(d.status.running, Kind::Motion, "motion from the first picture");
    let (w, h, px) = photo();
    settle(&mut d, &FrameRef::packed(w, h, PixelFormat::Rgb8, &px).unwrap());
    assert_eq!((d.status.running, d.status.stage), (Kind::Motion, "failed"));
    assert!(d.status.why.contains("motion stands in") && d.status.why.contains("SHA256"), "{}", d.status.why);
}

#[cfg(feature = "yolox")]
#[test]
fn yolox_takes_over_once_loaded_and_finds_the_people() {
    let mut d = Switching::new(Kind::Yolox, None);
    assert_eq!(d.status.running, Kind::Motion);
    assert!(d.status.why.contains("motion stands in while yolox"), "{}", d.status.why);
    let (w, h, px) = photo();
    let frame = FrameRef::packed(w, h, PixelFormat::Rgb8, &px).unwrap();
    let found = settle(&mut d, &frame);
    assert_eq!((d.status.running, d.status.stage), (Kind::Yolox, "ready"), "{}", d.status.why);
    let found = if found == 0 { d.detect(&frame).len() } else { found };
    assert!(found >= 2, "YOLOX finds the two people in the photograph, found {found}");
}
