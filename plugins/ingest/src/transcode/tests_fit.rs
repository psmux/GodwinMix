//! A scale node for each `fit`, built as the graph builds it and fed
//! pictures of another shape: it links, plays, and comes out the size asked
//! for with the picture laid in the way the fit says.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;
use serde_json::json;

use super::build::build;
use super::spec::NodeSpec;

/// `rate` false is a scale at the source's own rate, which has no
/// videorate: the crop then sits straight after the branch's queue.
fn node(fit: &str, rate: bool) -> NodeSpec {
    let raw = json!({"kind": "scale", "width": 320, "height": 180, "fps": [30, 1], "fit": fit, "border": 0xff00_0000u64, "deinterlace": "off", "rate": rate});
    NodeSpec { id: format!("scale:{fit}"), kind: "scale".into(), input: "decode".into(), raw }
}

/// One picture out of a 4:3 white source through the node for `fit`, at
/// the source's rate and at another: its size, and the luma of its top left
/// and centre pixels, which must be the same both ways.
fn through(fit: &str) -> ((i32, i32), u8, u8) {
    let kept = once(fit, false);
    assert_eq!(kept, once(fit, true), "{fit} at the source's rate and at another");
    kept
}

fn once(fit: &str, rate: bool) -> ((i32, i32), u8, u8) {
    gmx_netkit::init().unwrap();
    let built = build(&node(fit, rate)).unwrap_or_else(|e| panic!("{fit}: {e}"));
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("videotestsrc").property_from_str("pattern", "white").property("num-buffers", 5i32).build().unwrap();
    let shape = super::build::caps("video/x-raw,format=I420,width=240,height=180,framerate=30/1").unwrap();
    let sink = gst::ElementFactory::make("appsink").build().unwrap().downcast::<AppSink>().unwrap();
    sink.set_caps(Some(&"video/x-raw,format=I420".parse().unwrap()));
    sink.set_sync(false);
    pipeline.add_many([&src, &shape]).unwrap();
    src.link(&shape).unwrap();
    pipeline.add_many(built.elements.iter()).unwrap();
    pipeline.add(&sink).unwrap();
    gst::Element::link_many(built.elements.iter()).unwrap_or_else(|e| panic!("{fit}: the scale's own elements would not link: {e}"));
    shape.link(built.head.as_ref().unwrap()).unwrap();
    // The tee hands on whatever links it; a convert keeps I420 for the sink.
    let convert = gst::ElementFactory::make("videoconvert").build().unwrap();
    pipeline.add(&convert).unwrap();
    gst::Element::link_many([built.tee.as_ref().unwrap(), &convert, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(10)).unwrap_or_else(|| panic!("{fit}: no picture came out"));
    let _ = pipeline.set_state(gst::State::Null);
    let caps = sample.caps().unwrap().structure(0).unwrap().to_owned();
    let size = (caps.get::<i32>("width").unwrap(), caps.get::<i32>("height").unwrap());
    let map = sample.buffer().unwrap().map_readable().unwrap();
    let (w, h) = (size.0 as usize, size.1 as usize);
    (size, map[0], map[(h / 2) * w + w / 2])
}

#[test]
fn pad_lays_the_picture_in_with_bars() {
    let (size, corner, centre) = through("pad");
    assert_eq!(size, (320, 180));
    assert!(corner < 30, "the corner of a padded 4:3 picture is a black bar, luma {corner}");
    assert!(centre > 200, "the centre is the picture, luma {centre}");
}

#[test]
fn crop_fills_the_frame_and_cuts_what_overhangs() {
    let (size, corner, centre) = through("crop");
    assert_eq!(size, (320, 180));
    assert!(corner > 200, "a cropped picture fills the frame, corner luma {corner}");
    assert!(centre > 200);
}

#[test]
fn stretch_fills_the_frame_with_the_whole_picture() {
    let (size, corner, centre) = through("stretch");
    assert_eq!(size, (320, 180));
    assert!(corner > 200, "a stretched picture fills the frame, corner luma {corner}");
    assert!(centre > 200);
}
