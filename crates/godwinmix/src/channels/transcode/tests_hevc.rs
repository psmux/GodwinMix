//! HEVC in a channel's plan: an HEVC publisher is decoded for an H.264
//! destination, an H.264 publisher is encoded to HEVC for a destination that
//! asks, and AV1 is still refused with the reason.

use super::*;

fn hevc_live() -> Live {
    Live::from_plugin(&json!({
        "channel": "church", "app": "church", "stream": "main", "since_ms": 1,
        "video": {"codec": "h265", "width": 1920, "height": 1080, "fps": 30.0, "kbps": 6000, "frame_rate": 30.0},
        "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": 128},
    }))
    .map(|mut l| {
        l.channel = "church".into();
        l
    })
    .unwrap()
}

fn element(t: &Transcode, kind: &str) -> Value {
    let nodes = t.streams("church").unwrap()[0]["nodes"].clone();
    nodes.as_array().unwrap().iter().find(|n| n["kind"] == kind).unwrap()["element"].clone()
}

#[test]
fn an_hevc_publisher_is_decoded_for_an_h264_destination() {
    let t = Transcode::with(machine(false), governor(8));
    replan(&t, &[dest("a", Some(json!({"preset": "youtube-720p30"})))], &[hevc_live()]);
    assert!(t.view("church", "a").1.is_none(), "not refused: {:?}", t.view("church", "a").1);
    assert_eq!(element(&t, "decode"), "avdec_h265");
    assert_eq!(element(&t, "encode"), "x264enc");
}

#[test]
fn an_h264_publisher_is_encoded_to_hevc_when_a_destination_asks() {
    let t = Transcode::with(machine(false), governor(8));
    let ask = json!({"video": {"codec": "h265", "height": 720}});
    replan(&t, &[dest("a", Some(ask))], &[live(1920, 1080, 30.0, 6000)]);
    assert!(t.view("church", "a").1.is_none(), "not refused: {:?}", t.view("church", "a").1);
    assert_eq!(element(&t, "encode"), "x265enc");
}

#[test]
fn av1_is_refused_with_what_would_do() {
    let t = Transcode::with(machine(false), governor(8));
    let ask = json!({"video": {"codec": "av1", "height": 720}});
    replan(&t, &[dest("a", Some(ask))], &[live(1920, 1080, 30.0, 6000)]);
    let no = t.view("church", "a").1.expect("refused");
    // The planner says it first: FLV here carries H.264 and HEVC.
    assert!(no.message.contains("h264, h265"), "{}", no.message);
}
