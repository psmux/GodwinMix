//! The control for `frame_interval.rs`: the same take with the component
//! answering properly.
//!
//! Without this one, the fuel test would pass on a core where the transition
//! never worked at all. Here the curve reaches the pads, and the frame
//! interval is inside the same bar.
//!
//! A file of its own so it gets a process of its own. Two mixers in one
//! process tear down over each other's GStreamer elements.

mod support;

use godwinmix_core::observe::metrics;
use std::time::Duration;
use support::{clean, install, max_frame_interval, Core};

#[test]
#[ignore = "timing; run in release, see frame_interval.rs"]
fn a_component_that_answers_drives_the_pads_and_costs_no_frame_either() {
    godwinmix_wasm::install();
    let _ = gstreamer::init();
    let dir = install("curve");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        let mut core = Core::start(Default::default()).await;
        core.mixer.take_scene(core.scene("cam1"), None).expect("the first scene");
        tokio::time::sleep(Duration::from_millis(700)).await;

        metrics::reset_longest_frame_gap();
        core.ease_to("cam2", 300).expect("an ease");

        let driven = core.mixer.pool_for_tests().driven_by_a_transition("alpha");
        assert!(
            driven.contains(&"cam2".to_string()),
            "the curve never reached the incoming pad; driven: {driven:?}"
        );
        assert!(
            core.mixer.transition_window().is_some(),
            "the transition should be on the canvas"
        );

        tokio::time::sleep(Duration::from_millis(1_000)).await;
        let gap = metrics::longest_frame_gap();
        println!("longest programme frame interval: {} ms", gap.as_millis());
        assert!(gap > Duration::ZERO, "the programme probe saw no frames");
        assert!(
            gap <= max_frame_interval(),
            "the ease cost the programme a {} ms gap; the bar is {} ms",
            gap.as_millis(),
            max_frame_interval().as_millis()
        );
        core.shutdown();
    });
    clean(dir);
}
