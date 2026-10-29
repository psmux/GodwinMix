//! The colour correction in a real pipeline: canvas video in, the bin, an appsink.

use super::*;
use gstreamer_app as gst_app;

fn canvas() -> CanvasCaps {
    CanvasCaps { width: 64, height: 36, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 }
}

/// The first frame's Y, U and V at the top left, through `color/filter`.
fn first_pixel(params: &str) -> (u8, u8, u8) {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("videotestsrc").property("num-buffers", 1).property_from_str("pattern", "red").build().unwrap();
    let caps = gst::ElementFactory::make("capsfilter").property("caps", canvas().video()).build().unwrap();
    let mut filter = Colour::default();
    let bin = filter.build(&canvas(), &toml::from_str(params).unwrap()).unwrap();
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

#[test]
fn nothing_set_leaves_the_frame_as_it_was() {
    assert_eq!(first_pixel(""), first_pixel("brightness = 0"));
}

#[test]
fn no_saturation_turns_red_grey_in_the_pipeline() {
    let (_, u, v) = first_pixel("saturation = -1");
    assert_eq!((u, v), (128, 128));
}

#[test]
fn brightness_lifts_luma_in_the_pipeline() {
    let (y0, _, _) = first_pixel("");
    let (y1, _, _) = first_pixel("brightness = 0.3");
    assert!(y1 > y0 + 40, "{y0} -> {y1}");
}

#[test]
fn an_obs_value_out_of_range_names_the_field() {
    let e = validate(&toml::from_str("contrast = 9").unwrap()).unwrap_err().to_string();
    assert!(e.contains("color/filter params.contrast") && e.contains("-4 to 4"), "{e}");
}

/// What a full correction costs at 1080p, as a share of one core at 30
/// frames a second. A measurement, not a check:
///
/// ```text
/// cargo test -p godwinmix-core --release --lib cost_of_a_colour -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement; see the comment above"]
fn cost_of_a_colour_correction_at_1080p() {
    let _ = gst::init();
    let canvas = CanvasCaps { width: 1920, height: 1080, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    let frames = 600;
    let run = |params: Option<&str>| -> f64 {
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("videotestsrc").property("num-buffers", frames).property_from_str("pattern", "smpte").build().unwrap();
        let caps = gst::ElementFactory::make("capsfilter").property("caps", canvas.video()).build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").property("sync", false).build().unwrap();
        pipeline.add_many([&src, &caps, &sink]).unwrap();
        let mut chain = vec![src.clone(), caps.clone()];
        if let Some(p) = params {
            let bin = Colour::default().build(&canvas, &toml::from_str(p).unwrap()).unwrap();
            pipeline.add(&bin).unwrap();
            chain.push(bin);
        }
        chain.push(sink);
        gst::Element::link_many(&chain).unwrap();
        let started = std::time::Instant::now();
        pipeline.set_state(gst::State::Playing).unwrap();
        pipeline.bus().unwrap().timed_pop_filtered(gst::ClockTime::NONE, &[gst::MessageType::Eos, gst::MessageType::Error]);
        let took = started.elapsed().as_secs_f64();
        pipeline.set_state(gst::State::Null).unwrap();
        took
    };
    let bare = run(None);
    let seconds = frames as f64 / 30.0;
    for (label, p) in [("all zero", ""), ("brightness and contrast", "brightness = 0.1\ncontrast = 0.5"), ("everything", "brightness = 0.1\ncontrast = 0.5\ngamma = 0.3\nsaturation = 0.4\nhue_shift = 20")] {
        let took = run(Some(p));
        println!("{label:<26} {:>6.2}% of one core at 1080p30", (took - bare).max(0.0) / seconds * 100.0);
    }
}
