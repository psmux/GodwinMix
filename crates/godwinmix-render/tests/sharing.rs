//! Copy first, then decode once, scale once per shape, encode once per shape.

mod common;

use common::*;
use godwinmix_render::*;

#[test]
fn a_matching_source_is_copied_with_no_decode_and_no_encode() {
    let src = sources(&[("cam", h264_1080p30())]);
    let want = VideoWant {
        width: Some(1920),
        height: Some(1080),
        bitrate_kbps: Some(5500),
        ..VideoWant::default()
    };
    let reqs = on("cam", vec![with_video(request("yt", Container::Flv), want)]);
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert_eq!(plan.count(is_encode), 0);
    assert_eq!(plan.count(is_decode), 0);
    assert_eq!(
        plan.count(is_copy),
        2,
        "video and audio copied: {:#?}",
        plan.nodes
    );
    let mux = plan.output("yt").unwrap();
    assert_eq!(mux.reason.as_ref().unwrap().code, ReasonCode::Copied);
    assert_eq!(mux.inputs, vec!["copy:cam:video", "copy:cam:audio"]);
}

#[test]
fn an_empty_request_is_a_plain_copy() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(
        &src,
        &on("cam", vec![request("rec", Container::Mkv)]),
        &software(),
    )
    .unwrap();
    assert_eq!(plan.count(is_encode) + plan.count(is_audio_encode), 0);
}

#[test]
fn a_bitrate_outside_the_tolerance_is_an_encode() {
    let src = sources(&[("cam", h264_1080p30())]);
    let near = rung("near", 1080, 5000);
    let far = rung("far", 1080, 3000);
    let plan = plan(&src, &on("cam", vec![near, far]), &software()).unwrap();
    assert!(plan.chain("near").iter().any(|n| is_copy(&n.kind)));
    let encode = plan
        .chain("far")
        .into_iter()
        .find(|n| is_encode(&n.kind))
        .unwrap();
    assert_eq!(shape_of(encode).bitrate_kbps, 3000);
    let why = &plan.output("far").unwrap().reason.as_ref().unwrap().text;
    assert!(why.contains("6000 kbit/s"), "{why}");
}

#[test]
fn a_container_that_cannot_carry_the_source_codec_turns_a_copy_into_an_encode() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(
        &src,
        &on("cam", vec![request("web", Container::Webrtc)]),
        &software(),
    )
    .unwrap();
    // WebRTC carries H.264, so the video is copied; its sound must become Opus.
    assert!(plan.chain("web").iter().any(|n| matches!(
        n.kind,
        NodeKind::Copy {
            track: Track::Video,
            ..
        }
    )));
    assert_eq!(plan.count(is_audio_encode), 1);

    let vp9 = sources(&[("cam", encoded(VideoCodec::Vp9, 1280, 720, 30, 2500))]);
    let plan = godwinmix_render::plan(
        &vp9,
        &on("cam", vec![request("yt", Container::Flv)]),
        &software(),
    )
    .unwrap();
    let encode = plan
        .encodes()
        .next()
        .expect("VP9 cannot go in FLV, so it is encoded");
    assert_eq!(shape_of(encode).codec, VideoCodec::H264);
    let why = &plan.output("yt").unwrap().reason.as_ref().unwrap().text;
    assert!(why.contains("flv cannot carry the source's VP9"), "{why}");
}

#[test]
fn flv_carries_hevc_as_enhanced_rtmp_does() {
    let src = sources(&[("cam", encoded(VideoCodec::H265, 1920, 1080, 30, 4000))]);
    let plan = plan(
        &src,
        &on("cam", vec![request("yt", Container::Flv)]),
        &software(),
    )
    .unwrap();
    assert_eq!(plan.count(is_encode), 0);
}

#[test]
fn a_codec_the_container_cannot_carry_is_refused_with_the_ones_it_can() {
    let src = sources(&[("cam", h264_1080p30())]);
    let want = VideoWant {
        codec: Some(VideoCodec::Vp8),
        ..VideoWant::default()
    };
    let reqs = on("cam", vec![with_video(request("yt", Container::Flv), want)]);
    let err = plan(&src, &reqs, &software()).unwrap_err();
    assert_eq!(err.code(), "container-codec");
    assert!(err.to_string().contains("h264, h265"), "{err}");
    assert!(!err.to_string().contains("av1"), "no GStreamer muxer writes AV1 in FLV: {err}");
    assert_eq!(err.data()["allowed"][0], "h264");
}

#[test]
fn a_source_is_decoded_once_however_many_renditions_use_it() {
    let src = sources(&[("cam", h264_1080p30())]);
    let reqs = on(
        "cam",
        vec![
            rung("a", 720, 3000),
            rung("b", 480, 1200),
            rung("c", 360, 800),
        ],
    );
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert_eq!(
        plan.count(|k| matches!(
            k,
            NodeKind::Decode {
                track: Track::Video,
                ..
            }
        )),
        1
    );
    let decode = plan.node("decode:cam:video").unwrap();
    assert_eq!(decode.serves, vec!["a", "b", "c"]);
}

#[test]
fn a_size_is_scaled_once_and_an_encode_is_shared_by_every_output_that_wants_it() {
    let src = sources(&[("cam", h264_1080p30())]);
    let reqs = on(
        "cam",
        vec![
            rung("yt", 720, 3000),
            rung("fb", 720, 3000),
            rung("tw", 720, 3000),
            rung("low", 720, 1500),
        ],
    );
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert_eq!(plan.count(is_scale), 1, "one 720p scale for all four");
    assert_eq!(
        plan.count(is_encode),
        2,
        "3000k shared by three, 1500k alone"
    );
    let shared = plan
        .node("encode:cam:h264:1280x720p30:3000k:g2000")
        .unwrap();
    assert_eq!(shared.serves, vec!["yt", "fb", "tw"]);
    for out in ["yt", "fb", "tw"] {
        assert_eq!(plan.output(out).unwrap().inputs[0], shared.id);
    }
}

#[test]
fn a_raw_source_is_never_copied_and_never_decoded() {
    let src = sources(&[("programme", raw(1920, 1080, 30))]);
    let reqs = on(
        "programme",
        vec![request("yt", Container::Flv), rung("low", 720, 3000)],
    );
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert_eq!(plan.count(is_copy), 0);
    assert_eq!(plan.count(is_decode), 0);
    let full = plan
        .node("encode:programme:h264:1920x1080p30:6200k:g2000")
        .expect("default bitrate");
    assert_eq!(
        full.inputs,
        vec!["source:programme"],
        "same size, so no scale"
    );
}

#[test]
fn a_missing_width_follows_the_source_aspect_ratio() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(&src, &on("cam", vec![rung("a", 540, 2000)]), &software()).unwrap();
    let s = shape_of(plan.encodes().next().unwrap());
    assert_eq!((s.width, s.height), (960, 540));
}

#[test]
fn nodes_start_producers_first_and_cost_adds_up_per_device() {
    let src = sources(&[("cam", h264_1080p30())]);
    let plan = plan(&src, &on("cam", vec![rung("a", 720, 3000)]), &software()).unwrap();
    for (i, node) in plan.nodes.iter().enumerate() {
        for input in &node.inputs {
            let at = plan.nodes.iter().position(|n| &n.id == input).unwrap();
            assert!(at < i, "{} starts before its input {input}", node.id);
        }
    }
    let cpu = plan.cost[CPU];
    assert_eq!(cpu, plan.total);
    assert!(cpu.cpu_millicores > 500, "{cpu:?}");
    assert_eq!(cpu.egress_kbps, 3000 + 128);
}
