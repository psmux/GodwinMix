//! `image/source`: what claims an address, and the same conformance checks
//! every built in kind passes, run on a still and on a sequence the test
//! draws itself. A still that stalled after its one frame fails them.

use super::*;
use crate::config::SourceConfig;
use crate::plugin::harness::check_source;

#[test]
fn a_picture_address_is_claimed_above_a_clip() {
    assert_eq!(image_type("/srv/slides/title.PNG"), Some("image/png"));
    assert_eq!(image_type("https://cdn.example.com/logo.jpg?v=3"), Some("image/jpeg"));
    assert_eq!(image_type("/clips/ad.mp4"), None);
    let kind = |uri: &str| crate::plugin::source::resolve(uri).map(|p| p.manifest.provide_id());
    assert_eq!(kind("/srv/slides/title.png").as_deref(), Some("image/source"));
    assert_eq!(kind("file:///srv/slides/title.jpeg").as_deref(), Some("image/source"));
    assert_eq!(kind("/clips/ad.mp4").as_deref(), Some("file/source"));
    // Windows' extended form has a `?` that is part of the path. It was read
    // as a query, and a library picture became a clip that played once.
    assert_eq!(image_type(r"\\?\C:\Users\me\media\plate.png"), Some("image/png"));
    assert_eq!(kind(r"\\?\C:\Users\me\media\plate.png").as_deref(), Some("image/source"));
}

#[test]
fn a_numbered_pattern_is_a_sequence_and_fills_in() {
    assert!(is_sequence("/frames/f%04d.png"));
    assert!(is_sequence("/frames/f%d.jpg"));
    assert!(!is_sequence("/frames/100%.png"));
    assert!(!is_sequence("/frames/still.png"));
    assert_eq!(printf_d("/f/f%04d.png", 7), "/f/f0007.png");
    assert_eq!(printf_d("/f/shot%d.jpg", 12), "/f/shot12.jpg");
    let p: Params = toml::from_str("fps = 0").unwrap();
    assert!(fps(&p).unwrap_err().to_string().contains("1 to 120"));
}

fn draw(desc: &str) -> bool {
    gstreamer::parse::launch(desc)
        .ok()
        .and_then(|p| {
            p.set_state(gst::State::Playing).ok()?;
            let bus = p.bus()?;
            let done = bus.timed_pop_filtered(gst::ClockTime::from_seconds(10), &[gst::MessageType::Eos, gst::MessageType::Error]);
            let _ = p.set_state(gst::State::Null);
            done.filter(|m| m.type_() == gst::MessageType::Eos)
        })
        .is_some()
}

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gmx-image-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn a_still_passes_the_checks_every_kind_passes() {
    let _ = gst::init();
    let dir = scratch("still");
    let png = dir.join("title.png");
    if !draw(&format!("videotestsrc num-buffers=1 ! video/x-raw,width=640,height=360 ! pngenc ! filesink location=\"{}\"", png.display().to_string().replace('\\', "/"))) {
        println!("skipping: could not draw a PNG to check against");
        return;
    }
    let report = check_source(&SourceConfig::bare("harness-still", &png.to_string_lossy()), false).expect("the harness runs");
    report.lines().iter().for_each(|l| println!("{l}"));
    let _ = std::fs::remove_dir_all(&dir);
    report.into_result().expect("a still is conformant");
}

#[test]
fn a_sequence_passes_them_too() {
    let _ = gst::init();
    let dir = scratch("sequence");
    let pattern = dir.join("f%03d.jpg");
    let desc = format!("videotestsrc num-buffers=10 pattern=ball ! video/x-raw,width=640,height=360 ! jpegenc ! multifilesink location=\"{}\"", pattern.display().to_string().replace('\\', "/"));
    if !draw(&desc) {
        println!("skipping: could not draw a sequence to check against");
        return;
    }
    let report = check_source(&SourceConfig::bare("harness-sequence", &pattern.to_string_lossy()), false).expect("the harness runs");
    report.lines().iter().for_each(|l| println!("{l}"));
    let _ = std::fs::remove_dir_all(&dir);
    report.into_result().expect("a sequence is conformant");
}
