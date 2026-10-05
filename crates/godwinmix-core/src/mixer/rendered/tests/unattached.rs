//! An output the governor refused at start: kept, reported with why, saved,
//! and attached once there is room, with nobody asking for it again.

use super::super::super::Mixer;
use super::{config, encoders, output, want};
use godwinmix_govern::load::Load;
use godwinmix_protocol::types::OutputState;
use gstreamer as gst;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread")]
async fn an_output_refused_at_start_is_kept_and_attached_when_there_is_room() {
    let _ = gst::init();
    let mut cfg = config();
    cfg.outputs.push(output("archive", Some(want(144, 400))));
    let (mut mix, _h, _c, _b) = Mixer::build(cfg).expect("mixer builds");
    // Every core taken by other programs, as on a runner busy with the
    // other tests when a station starts a show again.
    let governor = mix.renditions().station().governor().clone();
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as u32;
    governor.load_cell().store(&Load { system_millicores: cores * 1000, others_peak_millicores: cores * 1000, samples: 5, ..Load::default() });
    mix.start().expect("the programme starts without the output");

    let status = mix.status();
    let archive = status.outputs.iter().find(|o| o.id == "archive").expect("the refused output is listed");
    assert_eq!(archive.state, OutputState::Failed);
    let why = archive.shed.clone().expect("with why");
    println!("waiting: {why}");
    assert!(why.contains("tried again"), "{why}");
    assert!(mix.runtime_configs().outputs.iter().any(|o| o.id == "archive"), "still saved for the next start");
    assert!(encoders(&mix).is_empty());

    // Still no room: asked again, refused again, still kept.
    tokio::time::sleep(Duration::from_millis(600)).await;
    mix.retry_unattached();
    assert!(encoders(&mix).is_empty() && mix.status().outputs.iter().any(|o| o.id == "archive"));

    // Room comes back. The second wait is longer than the first.
    governor.load_cell().store(&Load { samples: 6, ..Load::default() });
    tokio::time::sleep(Duration::from_millis(1500)).await;
    mix.retry_unattached();
    let status = mix.status();
    let archive = status.outputs.iter().find(|o| o.id == "archive").expect("listed once");
    assert!(archive.shed.is_none(), "{:?}", archive.shed);
    assert_eq!(status.outputs.iter().filter(|o| o.id == "archive").count(), 1);
    assert_eq!(encoders(&mix).len(), 1, "its rendition is running");
    mix.remove_output(&"archive".to_string()).unwrap();
    mix.shutdown();
}
