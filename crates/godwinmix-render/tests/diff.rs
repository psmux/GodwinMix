//! A running graph changes only what changed.

mod common;

use common::*;
use godwinmix_render::*;

fn ladder(heights: &[(u32, u32)]) -> Vec<(SourceId, RenditionRequest)> {
    on(
        "cam",
        heights
            .iter()
            .map(|(h, kbps)| rung(&format!("r{h}"), *h, *kbps))
            .collect(),
    )
}

#[test]
fn the_same_plan_twice_changes_nothing() {
    let src = sources(&[("cam", h264_1080p30())]);
    let a = plan(&src, &ladder(&[(720, 3000), (480, 1200)]), &software()).unwrap();
    let b = plan(&src, &ladder(&[(720, 3000), (480, 1200)]), &software()).unwrap();
    let d = diff(&a, &b);
    assert!(d.is_empty(), "{d:?}");
    assert_eq!(d.keep.len(), a.nodes.len());
}

#[test]
fn adding_a_rung_starts_its_scale_encode_and_mux_and_touches_nothing_else() {
    let src = sources(&[("cam", h264_1080p30())]);
    let before = plan(&src, &ladder(&[(720, 3000), (480, 1200)]), &software()).unwrap();
    let after = plan(
        &src,
        &ladder(&[(720, 3000), (480, 1200), (360, 800)]),
        &software(),
    )
    .unwrap();
    let d = diff(&before, &after);
    assert_eq!(
        d.start,
        vec![
            "scale:cam:640x360p30",
            "encode:cam:h264:640x360p30:800k:g2000",
            "mux:r360"
        ]
    );
    assert!(d.stop.is_empty() && d.restart.is_empty(), "{d:?}");
    assert_eq!(after.count(is_encode), before.count(is_encode) + 1);
}

#[test]
fn removing_the_last_output_on_an_encoder_stops_it_consumers_first() {
    let src = sources(&[("cam", h264_1080p30())]);
    let before = plan(&src, &ladder(&[(720, 3000), (480, 1200)]), &software()).unwrap();
    let after = plan(&src, &ladder(&[(720, 3000)]), &software()).unwrap();
    let d = diff(&before, &after);
    assert_eq!(
        d.stop,
        vec![
            "mux:r480",
            "encode:cam:h264:854x480p30:1200k:g2000",
            "scale:cam:854x480p30"
        ]
    );
    assert!(d.start.is_empty() && d.restart.is_empty(), "{d:?}");
}

#[test]
fn a_second_output_on_a_running_encoder_starts_only_its_mux() {
    let src = sources(&[("cam", h264_1080p30())]);
    let before = plan(&src, &on("cam", vec![rung("yt", 720, 3000)]), &software()).unwrap();
    let after = plan(
        &src,
        &on("cam", vec![rung("yt", 720, 3000), rung("fb", 720, 3000)]),
        &software(),
    )
    .unwrap();
    let d = diff(&before, &after);
    assert_eq!(d.start, vec!["mux:fb"]);
    assert!(d
        .keep
        .contains(&"encode:cam:h264:1280x720p30:3000k:g2000".to_string()));
}

#[test]
fn an_output_that_goes_from_copy_to_encode_restarts_its_mux() {
    let src = sources(&[("cam", h264_1080p30())]);
    let before = plan(
        &src,
        &on("cam", vec![request("yt", Container::Flv)]),
        &software(),
    )
    .unwrap();
    let after = plan(&src, &on("cam", vec![rung("yt", 720, 3000)]), &software()).unwrap();
    let d = diff(&before, &after);
    assert_eq!(d.restart, vec!["mux:yt"]);
    assert_eq!(d.stop, vec!["copy:cam:video"]);
    assert!(d.start.contains(&"decode:cam:video".to_string()));
}

#[test]
fn an_encoder_moved_off_the_gpu_is_restarted_under_the_same_id() {
    let src = sources(&[("cam", h264_1080p30())]);
    let gpu = software().with_hardware("h264-nvidia", VideoCodec::H264, "nvidia0");
    let full = gpu.clone().with_room(
        "nvidia0",
        Room {
            sessions: Some(0),
            device_millis: None,
        },
    );
    let before = plan(&src, &ladder(&[(720, 3000)]), &gpu).unwrap();
    let after = plan(&src, &ladder(&[(720, 3000)]), &full).unwrap();
    let d = diff(&before, &after);
    assert_eq!(d.restart, vec!["encode:cam:h264:1280x720p30:3000k:g2000"]);
}
