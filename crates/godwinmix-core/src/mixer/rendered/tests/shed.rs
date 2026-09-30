//! Live wins: a lower rung stopped when the machine is over its line, the
//! output marked with why, and brought back once there is room.

use super::{config, mixer, output, want};
use godwinmix_govern::load::Load;
use godwinmix_protocol::rendition::{PresetRef, RenditionChoice};

#[tokio::test(flavor = "multi_thread")]
async fn a_lower_rung_is_shed_when_the_machine_runs_short_and_comes_back() {
    let mut mix = mixer(config()).await;
    let ladder = RenditionChoice::Preset(PresetRef { preset: "abr-ladder-3".into() });
    // HLS is not this test's business: the core's own feed reads rung 0 and
    // the lower rungs are only planned, built and governed.
    mix.add_output(&output("hls", Some(ladder))).expect("a small ladder fits");
    mix.add_output(&output("steady", Some(want(144, 400)))).unwrap();
    let governor = mix.renditions().station().governor().clone();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u32;
    let over = Load { system_millicores: cores * 1000, samples: 5, ..Load::default() };
    governor.load_cell().store(&over);

    mix.rendition_tick();
    let status = mix.status();
    let hls = status.outputs.iter().find(|o| o.id == "hls").unwrap();
    let why = hls.shed.clone().expect("the ladder's output says a rung was shed");
    println!("shed: {why}");
    let steady = status.outputs.iter().find(|o| o.id == "steady").unwrap();
    assert!(steady.shed.is_none(), "a single rendition is its output's top rung and is never shed");
    let plan = mix.renditions().shared_view().read().clone();
    let shed: Vec<_> = plan.nodes.iter().filter(|n| n.shed.is_some()).collect();
    assert!(!shed.is_empty() && shed.iter().all(|n| !n.serves.contains(&"hls-720p".to_string())), "the top rung stays: {shed:?}");

    // Room again; nothing comes back before the hold is over.
    governor.load_cell().store(&Load { samples: 6, ..Load::default() });
    mix.rendition_tick();
    assert!(mix.status().outputs.iter().any(|o| o.shed.is_some()), "back too soon");
    std::thread::sleep(crate::render::RESTORE_AFTER);
    mix.rendition_tick();
    assert!(mix.status().outputs.iter().all(|o| o.shed.is_none()), "brought back once there was room");
    mix.shutdown();
}
