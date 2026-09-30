//! Keyframes aligned across a ladder, and audio planned like video.

mod common;

use common::*;
use godwinmix_render::*;

fn keyframed(id: &str, height: u32, kbps: u32, keyframe_ms: Option<u32>) -> RenditionRequest {
    let mut req = rung(id, height, kbps);
    req.container = Container::Hls;
    req.video.as_mut().unwrap().keyframe_ms = keyframe_ms;
    req
}

#[test]
fn every_rung_of_one_source_gets_the_shortest_interval_asked_for() {
    let src = sources(&[("cam", h264_1080p30()), ("other", h264_1080p30())]);
    let mut reqs = on(
        "cam",
        vec![
            keyframed("r720", 720, 3000, Some(4000)),
            keyframed("r480", 480, 1200, Some(1000)),
            keyframed("r360", 360, 800, None),
        ],
    );
    reqs.extend(on("other", vec![keyframed("o720", 720, 3000, None)]));
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert_eq!(plan.keyframe_ms["cam"], 1000);
    assert_eq!(
        plan.keyframe_ms["other"], DEFAULT_KEYFRAME_MS,
        "a source nobody asked about gets the default"
    );
    for node in plan.encodes() {
        let NodeKind::Encode { source, shape, .. } = &node.kind else {
            unreachable!()
        };
        assert_eq!(shape.keyframe_ms, plan.keyframe_ms[source], "{}", node.id);
    }
}

#[test]
fn a_copied_rung_pulls_the_ladder_to_its_own_interval() {
    let mut info = h264_1080p30();
    info.video.as_mut().unwrap().keyframe_ms = 1000;
    let src = sources(&[("cam", info)]);
    let reqs = on(
        "cam",
        vec![
            request("top", Container::Hls),
            keyframed("r720", 720, 3000, None),
        ],
    );
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert!(plan.chain("top").iter().any(|n| is_copy(&n.kind)));
    assert_eq!(plan.keyframe_ms["cam"], 1000);
}

#[test]
fn a_different_interval_from_the_source_is_an_encode() {
    let src = sources(&[("cam", h264_1080p30())]);
    let want = VideoWant {
        keyframe_ms: Some(4000),
        ..VideoWant::default()
    };
    let plan = plan(
        &src,
        &on("cam", vec![with_video(request("a", Container::Flv), want)]),
        &software(),
    )
    .unwrap();
    assert_eq!(plan.count(is_encode), 1);
}

#[test]
fn audio_is_copied_when_it_matches_and_encoded_once_per_shape_when_not() {
    let src = sources(&[("cam", h264_1080p30())]);
    let opus = |id: &str| RenditionRequest {
        no_video: true,
        ..request(id, Container::Webrtc)
    };
    let mono = |id: &str| RenditionRequest {
        audio: Some(AudioWant {
            channels: Some(1),
            ..AudioWant::default()
        }),
        ..request(id, Container::Flv)
    };
    let reqs = on(
        "cam",
        vec![
            request("yt", Container::Flv),
            opus("w1"),
            opus("w2"),
            mono("m"),
        ],
    );
    let plan = plan(&src, &reqs, &software()).unwrap();
    assert!(plan.chain("yt").iter().any(|n| matches!(
        n.kind,
        NodeKind::Copy {
            track: Track::Audio,
            ..
        }
    )));
    assert_eq!(
        plan.count(|k| matches!(
            k,
            NodeKind::Decode {
                track: Track::Audio,
                ..
            }
        )),
        1
    );
    assert_eq!(
        plan.count(is_audio_encode),
        2,
        "one Opus shared by two, one mono AAC"
    );
    let opus = plan.node("aencode:cam:opus:2ch48000:96k").unwrap();
    assert_eq!(opus.serves, vec!["w1", "w2"]);
    assert_eq!(
        plan.count(|k| matches!(k, NodeKind::AudioConvert { .. })),
        1
    );
    assert_eq!(
        plan.output("w1").unwrap().inputs,
        vec![opus.id.clone()],
        "no video in an audio only output"
    );
}

#[test]
fn an_audio_codec_the_machine_lacks_names_the_one_it_has() {
    let src = sources(&[("cam", h264_1080p30())]);
    let want = AudioWant {
        codec: Some(AudioCodec::Ac3),
        ..AudioWant::default()
    };
    let req = RenditionRequest {
        audio: Some(want),
        ..request("ts", Container::MpegTs)
    };
    let err = plan(&src, &on("cam", vec![req]), &software()).unwrap_err();
    assert_eq!(err.code(), "no-audio-encoder");
    assert_eq!(err.data()["nearest"], "AAC");
    assert!(err.to_string().contains("AAC is possible"), "{err}");
}

#[test]
fn a_track_the_source_lacks_is_refused() {
    let mut info = h264_1080p30();
    info.audio = None;
    let src = sources(&[("cam", info)]);
    let req = RenditionRequest {
        audio: Some(AudioWant::default()),
        ..request("a", Container::Flv)
    };
    let err = plan(&src, &on("cam", vec![req]), &software()).unwrap_err();
    assert_eq!(err.code(), "missing-track");
    // With nothing asked of the audio, a silent source is planned as video only.
    let plan = plan(
        &src,
        &on("cam", vec![request("b", Container::Flv)]),
        &software(),
    )
    .unwrap();
    assert_eq!(plan.output("b").unwrap().inputs, vec!["copy:cam:video"]);
}

#[test]
fn unknown_sources_duplicates_and_empty_requests_are_refused_with_next_steps() {
    let src = sources(&[("cam", h264_1080p30())]);
    let err = plan(
        &src,
        &on("nope", vec![request("a", Container::Flv)]),
        &software(),
    )
    .unwrap_err();
    assert_eq!(err.code(), "unknown-source");
    assert!(err.to_string().contains("Pick one of: cam"), "{err}");
    let twice = on(
        "cam",
        vec![request("a", Container::Flv), request("a", Container::Mkv)],
    );
    assert_eq!(
        plan(&src, &twice, &software()).unwrap_err().code(),
        "duplicate-request"
    );
    let empty = RenditionRequest {
        no_video: true,
        no_audio: true,
        ..request("a", Container::Flv)
    };
    assert_eq!(
        plan(&src, &on("cam", vec![empty]), &software())
            .unwrap_err()
            .code(),
        "nothing-asked"
    );
}
