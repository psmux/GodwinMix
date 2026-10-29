//! Several filters at one insertion point: each runs after the one before it,
//! and any of them comes out without taking the others with it.

use super::*;
use crate::config::Params;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// A live tone through the canvas audio caps into a stand in for the proxy,
/// with the loudest sample of each buffer kept as the sink sees it.
fn audio_stand_in() -> (gst::Pipeline, gst::Element, gst::Element, Arc<AtomicU32>) {
    let _ = gst::init();
    let canvas = crate::plugin::harness::test_canvas();
    let pipeline = gst::Pipeline::with_name("filter-chain-test");
    let src = crate::gstutil::make("audiotestsrc", "tone").unwrap();
    src.set_property("is-live", true);
    src.set_property("volume", 0.8f64);
    let caps = crate::gstutil::capsfilter("acaps", &canvas.audio()).unwrap();
    let proxy = crate::gstutil::make("identity", "aproxy").unwrap();
    let sink = crate::gstutil::make("fakesink", "out").unwrap();
    sink.set_property("sync", false);
    pipeline.add_many([&src, &caps, &proxy, &sink]).unwrap();
    gst::Element::link_many([&src, &caps, &proxy, &sink]).unwrap();
    let peak = Arc::new(AtomicU32::new(0));
    let seen = peak.clone();
    sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_p, info| {
        if let Some(gst::PadProbeData::Buffer(b)) = &info.data {
            if let Ok(map) = b.map_readable() {
                let loudest = map.as_slice().chunks_exact(2).map(|s| i16::from_le_bytes([s[0], s[1]]).unsigned_abs()).max();
                seen.store(loudest.unwrap_or(0) as u32, Ordering::Relaxed);
            }
        }
        gst::PadProbeReturn::Ok
    });
    (pipeline, caps, proxy, peak)
}

fn gain(pipeline: &gst::Pipeline, caps: &gst::Element, proxy: &gst::Element, id: &str) -> FilterSlot {
    let canvas = crate::plugin::harness::test_canvas();
    let params: Params = toml::from_str("gain_db = -6.0206").unwrap();
    insert(
        Insertion::between(pipeline, caps, proxy),
        FilterSpec { id: id.into(), type_id: "gain/filter".into(), side: FilterSide::SourceInput, params },
        make("gain/filter").unwrap(),
        &canvas,
        true,
    )
    .unwrap_or_else(|e| panic!("{id} goes in live: {e:#}"))
}

/// Wait until the level at the sink settles near `want` of full volume.
fn settles_at(peak: &AtomicU32, want: f64, what: &str) {
    let full = 0.8 * 32767.0;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let now = peak.load(Ordering::Relaxed) as f64 / full;
        if (now - want).abs() < 0.03 {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: the level is {now:.3} of full, not {want}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn two_filters_on_one_point_run_in_turn_and_either_comes_out_alone() {
    let (pipeline, caps, proxy, peak) = audio_stand_in();
    pipeline.set_state(gst::State::Playing).unwrap();
    settles_at(&peak, 1.0, "with no filter");

    let first = gain(&pipeline, &caps, &proxy, "first");
    settles_at(&peak, 0.5, "with one gain of -6 dB");
    let second = gain(&pipeline, &caps, &proxy, "second");
    settles_at(&peak, 0.25, "with two in a row");

    // The first one, from the top of the chain: the second must stay in.
    first.remove().expect("the first comes out");
    settles_at(&peak, 0.5, "with the second alone");
    second.remove().expect("the second comes out");
    settles_at(&peak, 1.0, "with both gone");
    let _ = pipeline.set_state(gst::State::Null);
}
