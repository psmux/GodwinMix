//! The cutout through a real pipeline: real GStreamer, the real runtime, the
//! real models. A moving test pattern has no person in it, so once the model
//! has answered, what the cutout keeps is next to nothing and the frame goes
//! to black. Skips, and says why, on a machine with no runtime or no models;
//! `dev/fetch-models.sh` puts both in place.
#![cfg(feature = "matte")]

use godwinmix_core::plugin::filter::Filter;
use godwinmix_core::plugin::filters::matte;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::time::{Duration, Instant};

const W: u32 = 640;
const H: u32 = 360;

fn mean_luma(sample: &gst::Sample) -> f64 {
    let buffer = sample.buffer().unwrap();
    let map = buffer.map_readable().unwrap();
    let y = &map[..(W * H) as usize];
    y.iter().map(|&v| v as f64).sum::<f64>() / y.len() as f64
}

fn run(quality: &str, device: &str) -> Option<(f64, f64, Duration)> {
    let mut params = toml::Table::new();
    params.insert("quality".into(), quality.into());
    params.insert("device".into(), device.into());
    match matte::ready(&params) {
        Ok(what) => eprintln!("{quality} on {device}: {what}, accelerators {:?}", matte::runtime::accelerators().iter().map(|a| a.0).collect::<Vec<_>>()),
        Err(e) => {
            eprintln!("skipped {quality} on {device}: {e:#}");
            return None;
        }
    }
    gst::init().unwrap();
    let canvas = godwinmix_core::caps::CanvasCaps { width: W as i32, height: H as i32, fps: gst::Fraction::new(30, 1), sample_rate: 48000, channels: 2 };
    let mut cutout = matte::Cutout::default();
    let filter = cutout.build(&canvas, &params).expect("the cutout builds");
    let pipeline = gst::parse::launch(&format!(
        "videotestsrc is-live=true pattern=smpte ! video/x-raw,format=I420,width={W},height={H},framerate=30/1 ! \
         identity name=in ! appsink name=out sync=false max-buffers=2 drop=true"
    ))
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    // The cutout goes between `in` and the sink, the way a filter goes in.
    let (inp, out) = (pipeline.by_name("in").unwrap(), pipeline.by_name("out").unwrap());
    inp.unlink(&out);
    pipeline.add(&filter).unwrap();
    inp.link(&filter).unwrap();
    filter.link(&out).unwrap();
    let sink = out.downcast::<gst_app::AppSink>().unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    let first = sink.try_pull_sample(gst::ClockTime::from_seconds(10)).expect("a first frame");
    let before = mean_luma(&first);
    // Long enough for the model to load, a GPU to prepare and an answer to
    // come back; a cutout that never answers leaves the pattern as it was.
    let started = Instant::now();
    let mut after = before;
    while started.elapsed() < Duration::from_secs(30) {
        let Some(s) = sink.try_pull_sample(gst::ClockTime::from_seconds(2)) else { continue };
        after = mean_luma(&s);
        if after < 40.0 {
            break;
        }
    }
    let took = started.elapsed();
    let _ = pipeline.set_state(gst::State::Null);
    Some((before, after, took))
}

#[test]
fn a_frame_with_nobody_in_it_is_cut_out_to_black_on_the_cpu() {
    let Some((before, after, took)) = run("fast", "cpu") else { return };
    eprintln!("fast on cpu: mean luma {before:.0} -> {after:.0} in {took:?}");
    assert!(before > 80.0, "the pattern is bright to start with: {before}");
    assert!(after < 40.0, "the cutout took nothing away: {before} -> {after}");
}

#[test]
fn the_fine_model_runs_on_whatever_the_machine_has() {
    let Some((before, after, took)) = run("fine", "auto") else { return };
    eprintln!("fine on auto: mean luma {before:.0} -> {after:.0} in {took:?}");
    assert!(after < 40.0, "the cutout took nothing away: {before} -> {after}");
}

#[test]
fn a_missing_model_is_named_with_where_it_was_looked_for() {
    let mut params = toml::Table::new();
    params.insert("model".into(), "no_such_model".into());
    let Err(e) = matte::ready(&params) else { panic!("a model that is not there was ready") };
    let text = format!("{e:#}");
    assert!(text.contains("no_such_model.onnx") || text.contains("ONNX Runtime"), "{text}");
}
