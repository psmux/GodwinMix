//! The sound filters in a real pipeline: canvas audio in, the bin, an appsink.

use super::*;
use gstreamer_app as gst_app;

fn canvas() -> CanvasCaps {
    CanvasCaps { width: 320, height: 180, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 }
}

/// The loudest sample out of a second of a full scale sine through `type_id`.
fn peak_through(type_id: &str, params: &str) -> i16 {
    let _ = gst::init();
    let pipeline = gst::Pipeline::new();
    let src = gst::ElementFactory::make("audiotestsrc")
        .property("num-buffers", 50)
        .property("volume", 1.0f64)
        .property("samplesperbuffer", 960i32)
        .build()
        .unwrap();
    let caps = gst::ElementFactory::make("capsfilter").property("caps", canvas().audio()).build().unwrap();
    let mut filter = make_filter(type_id).expect("a sound filter");
    let bin = filter.build(&canvas(), &toml::from_str(params).unwrap()).unwrap();
    let sink = gst_app::AppSink::builder().sync(false).build();
    pipeline.add_many([&src, &caps, &bin, sink.upcast_ref()]).unwrap();
    gst::Element::link_many([&src, &caps, &bin, sink.upcast_ref()]).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let mut peak = 0i16;
    while let Ok(sample) = sink.pull_sample() {
        let buffer = sample.buffer().unwrap();
        let map = buffer.map_readable().unwrap();
        for b in map.as_slice().chunks_exact(2) {
            peak = peak.max(i16::from_le_bytes([b[0], b[1]]).saturating_abs());
        }
    }
    pipeline.set_state(gst::State::Null).unwrap();
    peak
}

#[test]
fn every_sound_filter_in_this_build_is_listed_and_made() {
    for m in built_in() {
        let id = m.provide_id();
        assert!(is_audio(&id), "{id}");
        let f = make_filter(&id).unwrap();
        assert_eq!(f.stream(), Stream::Audio, "{id}");
    }
}

#[test]
fn a_gain_filter_turns_the_sound_down_in_the_pipeline() {
    let peak = peak_through("gain/filter", "gain_db = -12");
    let want = (32767.0 * db_to_gain_f(-12.0)) as i16;
    assert!((peak - want).abs() < 200, "peak {peak} want about {want}");
}

#[test]
fn a_limiter_holds_the_programme_under_its_ceiling() {
    let peak = peak_through("limiter/filter", "threshold_db = -6");
    assert!(peak <= (32768.0 * db_to_gain_f(-6.0)) as i16 + 1, "{peak}");
}

#[test]
fn bypass_lets_the_sound_through_untouched() {
    let peak = peak_through("gain/filter", "gain_db = -30\nbypass = true");
    assert!(peak > 32000, "{peak}");
}

#[test]
fn a_bad_setting_is_refused_before_anything_is_built() {
    let e = validate("compressor/filter", &toml::from_str("ratio = 0.5").unwrap()).unwrap_err().to_string();
    assert!(e.contains("compressor/filter params.ratio"), "{e}");
    assert!(validate("chroma/filter", &toml::from_str("ratio = 0.5").unwrap()).is_ok());
}

fn db_to_gain_f(db: f64) -> f64 {
    10f64.powf(db / 20.0)
}

/// What each filter costs, as a share of one core: a minute of canvas audio
/// pushed through it as fast as it goes, against the same minute with no
/// filter. A measurement, not a check:
///
/// ```text
/// cargo test -p godwinmix-core --release --features denoise --lib cost_of_each -- --ignored --nocapture
/// ```
#[test]
#[ignore = "a measurement; see the comment above"]
fn cost_of_each_sound_filter() {
    let _ = gst::init();
    let seconds = 60.0;
    let run = |type_id: Option<&str>| -> f64 {
        let pipeline = gst::Pipeline::new();
        let src = gst::ElementFactory::make("audiotestsrc")
            .property("num-buffers", (seconds * 50.0) as i32)
            .property("samplesperbuffer", 960i32)
            .property_from_str("wave", "pink-noise")
            .build()
            .unwrap();
        let caps = gst::ElementFactory::make("capsfilter").property("caps", canvas().audio()).build().unwrap();
        let sink = gst::ElementFactory::make("fakesink").property("sync", false).build().unwrap();
        pipeline.add_many([&src, &caps, &sink]).unwrap();
        let mut chain = vec![src.clone(), caps.clone()];
        if let Some(t) = type_id {
            let bin = make_filter(t).unwrap().build(&canvas(), &Params::new()).unwrap();
            pipeline.add(&bin).unwrap();
            chain.push(bin);
        }
        chain.push(sink);
        gst::Element::link_many(&chain).unwrap();
        let started = std::time::Instant::now();
        pipeline.set_state(gst::State::Playing).unwrap();
        let bus = pipeline.bus().unwrap();
        bus.timed_pop_filtered(gst::ClockTime::NONE, &[gst::MessageType::Eos, gst::MessageType::Error]);
        let took = started.elapsed().as_secs_f64();
        pipeline.set_state(gst::State::Null).unwrap();
        took
    };
    let bare = run(None);
    println!("48 kHz stereo, {seconds} s of pink noise; share of one core over the bare pipeline");
    for m in built_in() {
        let id = m.provide_id();
        let took = run(Some(&id));
        println!("{id:<20} {:>6.2}%", (took - bare).max(0.0) / seconds * 100.0);
    }
}
