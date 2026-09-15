//! The acceptance test for the tier: a component that runs out of fuel in the
//! middle of a take does not move the programme's frame interval.
//!
//! This is the whole argument for why tier W is allowed to exist. A transition
//! component is asked for its answer before the transition window opens, on a
//! worker thread, with a fuel allowance. When it runs out, the take falls back
//! to the built in cut and the compositor never notices. The measurement is
//! `observe::metrics::longest_frame_gap`, the same number Phase 6's other
//! acceptance criteria are stated against, and the bar is the same 34 ms.
//!
//! Ignored by default, like the other timing acceptance tests in `mixer.rs`:
//! a debug build on a loaded machine misses a frame for reasons that have
//! nothing to do with what is being measured. Run it the way the number is
//! quoted:
//!
//! ```text
//! cargo test --release -p godwinmix-wasm --test frame_interval -- --ignored
//! ```

mod support;

use godwinmix_core::observe::metrics;
use std::time::Duration;
use support::{clean, install, Core, MAX_FRAME_INTERVAL};

#[test]
#[ignore = "timing; run in release, see the module docs"]
fn a_render_cut_by_its_fuel_limit_does_not_move_the_programmes_frame_interval() {
    godwinmix_wasm::install();
    let _ = gstreamer::init();
    let dir = install("starved");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("a runtime");
    runtime.block_on(async {
        // A thousand operations is enough to enter the export and not enough
        // to leave it, so every `render` traps on the way through.
        let mut core = Core::start(support::fuel(1_000)).await;
        core.mixer.take_scene(core.scene("cam1"), None).expect("the first scene");
        // Long enough for the pipeline to be producing steadily, so the reset
        // below is taken from a settled programme and not from a preroll.
        tokio::time::sleep(Duration::from_millis(700)).await;

        metrics::reset_longest_frame_gap();
        core.ease_to("cam2", 400).expect("the take lands even though the component cannot answer");
        tokio::time::sleep(Duration::from_millis(1_000)).await;

        let gap = metrics::longest_frame_gap();
        println!("longest programme frame interval: {} ms", gap.as_millis());
        assert!(gap > Duration::ZERO, "the programme probe saw no frames, so this measured nothing");
        assert!(
            gap <= MAX_FRAME_INTERVAL,
            "a component that ran out of fuel cost the programme a {} ms gap; the bar is {} ms",
            gap.as_millis(),
            MAX_FRAME_INTERVAL.as_millis()
        );
        // And the take still happened: a transition that cannot be described
        // is a cut, not a refusal, so the programme is on the new scene.
        assert_eq!(
            core.mixer.status().program.as_deref(),
            Some("cam2"),
            "the take must land as a cut when the transition cannot be described"
        );
        assert!(
            core.mixer.pool_for_tests().driven_by_a_transition("alpha").is_empty(),
            "nothing should be driven: the component never answered"
        );
        core.shutdown();
    });
    clean(dir);
}
