//! Which encoder each Encode node gets, and why.

mod common;

use common::*;
use godwinmix_render::*;

fn with_gpu() -> StaticCostModel {
    software()
        .with_hardware("h265-nvidia", VideoCodec::H265, "nvidia0")
        .with_hardware("h264-nvidia", VideoCodec::H264, "nvidia0")
}

#[test]
fn hardware_of_the_right_codec_is_preferred() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(&src, &on("cam", vec![rung("a", 720, 3000)]), &with_gpu()).unwrap();
    let node = plan.encodes().next().unwrap();
    assert_eq!(encoder_of(node).id, "h264-nvidia");
    assert_eq!(node.device, "nvidia0");
    let reason = node.reason.as_ref().unwrap();
    assert_eq!(reason.code, ReasonCode::Hardware);
    assert!(plan.cost["nvidia0"].device_sessions == 1, "{:?}", plan.cost);
}

#[test]
fn software_is_used_when_there_is_no_hardware_and_says_so() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(&src, &on("cam", vec![rung("a", 720, 3000)]), &software()).unwrap();
    let reason = plan.encodes().next().unwrap().reason.clone().unwrap();
    assert_eq!(reason.code, ReasonCode::SoftwareOnly);
    assert_eq!(
        reason.text,
        "using h264-software-x264 because this machine has no hardware H.264 encoder"
    );
}

#[test]
fn a_full_gpu_is_passed_over_for_software_and_the_reason_names_it() {
    let model = with_gpu().with_room(
        "nvidia0",
        Room {
            sessions: Some(2),
            device_millis: None,
        },
    );
    let src = sources(&[("cam", h264_1080p30())]);
    let reqs = on(
        "cam",
        vec![
            rung("a", 720, 3000),
            rung("b", 480, 1200),
            rung("c", 360, 800),
        ],
    );
    let plan = plan(&src, &reqs, &model).unwrap();
    let chosen: Vec<&str> = plan.encodes().map(|n| encoder_of(n).id.as_str()).collect();
    assert_eq!(
        chosen,
        vec!["h264-nvidia", "h264-nvidia", "h264-software-x264"]
    );
    let last = plan.encodes().last().unwrap().reason.clone().unwrap();
    assert_eq!(last.code, ReasonCode::DeviceFull);
    assert_eq!(
        last.text,
        "using h264-software-x264 because the GPU nvidia0 is full"
    );
    assert_eq!(plan.cost["nvidia0"].device_sessions, 2);
}

#[test]
fn a_gpu_with_no_room_at_all_is_never_used() {
    let model = with_gpu().with_room(
        "nvidia0",
        Room {
            sessions: None,
            device_millis: Some(0),
        },
    );
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(&src, &on("cam", vec![rung("a", 720, 3000)]), &model).unwrap();
    assert!(!encoder_of(plan.encodes().next().unwrap()).hardware);
    assert!(!plan.cost.contains_key("nvidia0"));
}

#[test]
fn a_shape_the_hardware_cannot_make_goes_to_software() {
    let src = sources(&[("cam", encoded(VideoCodec::H264, 7680, 4320, 30, 80_000))]);
    let want = VideoWant {
        bitrate_kbps: Some(40_000),
        ..VideoWant::default()
    };
    let reqs = on("cam", vec![with_video(request("a", Container::Mkv), want)]);
    let plan = plan(&src, &reqs, &with_gpu()).unwrap();
    let reason = plan.encodes().next().unwrap().reason.clone().unwrap();
    assert_eq!(reason.code, ReasonCode::ShapeUnsupported);
    assert!(
        reason
            .text
            .contains("h264-nvidia cannot make 7680x4320 at 30 fps"),
        "{}",
        reason.text
    );
}

#[test]
fn a_request_nothing_can_make_names_what_is_missing_and_what_is_possible() {
    let model = software().without(VideoCodec::H265);
    let src = sources(&[("cam", h264_1080p30())]);
    let want = VideoWant {
        codec: Some(VideoCodec::H265),
        height: Some(720),
        ..VideoWant::default()
    };
    let err = plan(
        &src,
        &on("cam", vec![with_video(request("a", Container::Flv), want)]),
        &model,
    )
    .unwrap_err();
    assert_eq!(err.code(), "no-encoder");
    let data = err.data();
    assert_eq!(data["codec"], "h265");
    assert_eq!(data["nearest"]["codec"], "h264");
    assert_eq!(data["nearest"]["encoder"], "h264-software-x264");
    assert!(
        err.to_string()
            .contains("H.264 1280x720 at 30 fps with h264-software-x264 is possible"),
        "{err}"
    );
}

#[test]
fn the_nearest_shape_is_smaller_when_only_a_smaller_one_fits() {
    let model = StaticCostModel {
        encoders: Vec::new(),
        ..software()
    }
    .with_hardware("h264-hw", VideoCodec::H264, "gpu0");
    let src = sources(&[("cam", encoded(VideoCodec::H264, 7680, 4320, 60, 80_000))]);
    let want = VideoWant {
        bitrate_kbps: Some(40_000),
        ..VideoWant::default()
    };
    let err = plan(
        &src,
        &on("cam", vec![with_video(request("a", Container::Mkv), want)]),
        &model,
    )
    .unwrap_err();
    let data = err.data();
    assert_eq!(data["nearest"]["height"], 2160, "{data}");
    assert_eq!(data["nearest"]["fps"]["num"], 60);
    assert_eq!(data["tried"][0]["encoder"], "h264-hw");
}

#[test]
fn an_inherited_codec_this_machine_cannot_encode_falls_back_to_the_container_default() {
    // MKV carries ProRes, so a copy would keep it; at another size it must
    // be encoded, and nothing here encodes ProRes.
    let src = sources(&[("cam", encoded(VideoCodec::Prores, 1920, 1080, 30, 150_000))]);
    let mut req = rung("a", 720, 3000);
    req.container = Container::Mkv;
    let plan = plan(&src, &on("cam", vec![req]), &software()).unwrap();
    assert_eq!(
        shape_of(plan.encodes().next().unwrap()).codec,
        VideoCodec::H264
    );
    // Asked for by name, it is refused instead.
    let want = VideoWant {
        codec: Some(VideoCodec::Prores),
        height: Some(720),
        ..VideoWant::default()
    };
    let err = godwinmix_render::plan(
        &src,
        &on("cam", vec![with_video(request("b", Container::Mkv), want)]),
        &software(),
    );
    assert_eq!(err.unwrap_err().code(), "no-encoder");
}
