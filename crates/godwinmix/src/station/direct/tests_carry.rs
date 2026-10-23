//! A headend import's channel, an MPEG-2 programme into an HLS output that
//! converts, whose first word on the input has sound and no picture yet.
//! The input must keep carrying MPEG-2 whatever the plan said of that first
//! word, and the show is planned again when the picture comes.

use crate::station::direct::take_event;
use crate::station::registry::{OutputRecord, Record, Registry};
use crate::station::state::{Launch, Station};
use godwinmix_core::mixer::MixerHandle;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::shows::InputSpec;
use serde_json::{json, Value};
use std::sync::Arc;

fn station() -> Arc<Station> {
    gstreamer::init().unwrap();
    let dir = std::env::temp_dir().join(format!("gmx-direct-carry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut reg = Registry::open(&dir.join("godwinmix.toml")).unwrap();
    let mut ch = Record::new("ch117", "CH117", None);
    ch.compositing = false;
    ch.input = Some(InputSpec { uri: "udp://@239.1.1.1:5000".into(), program: Some(7), params: None, backup: None });
    let ask = json!({"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500, "keyframe_frames": 50, "fit": "pad"},
                     "audio": {"codec": "aac", "bitrate_kbps": 128, "tracks": "all"}});
    ch.outputs.push(OutputRecord {
        id: "flussonic".into(),
        platform: "hls".into(),
        label: "flussonic".into(),
        uri_host: "hls://flussonic".into(),
        has_key: false,
        enabled: true,
        rendition: Some(serde_json::from_value(ask).unwrap()),
    });
    reg.records.push(ch);
    let (events, _commands) = MixerHandle::detached();
    let governor = Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 8, 16_384);
    let render = godwinmix_core::render::Station::with_governor(governor, &godwinmix_core::catalogue::global(), Default::default());
    let launch = Launch { exe: "none".into(), common: Vec::new(), calibration: None };
    let tokens = Arc::new(godwinmix_protocol::scope::Tokens::new(Vec::new(), false));
    Station::new(reg, events, tokens, render, launch)
}

fn row(st: &Station) -> Value {
    st.direct.table(st)[0].clone()
}

fn input(video: bool, tracks: usize) -> Value {
    let mut v = json!({"show": "ch117", "state": "live", "since_ms": 1, "relay": "127.0.0.1:1", "stream": "direct.ch117/main",
                       "audio": {"codec": "ac3", "channels": 2, "sample_rate": 48000, "kbps": 192},
                       "tracks": (0..=tracks).map(|t| json!({"track": t, "kind": "audio", "codec": "ac3", "channels": 2, "sample_rate": 48000, "kbps": 192})).collect::<Vec<_>>()});
    if video {
        v["video"] = json!({"codec": "mpeg2", "width": 704, "height": 480, "fps": 29.97, "frame_rate": 29.97, "interlaced": true, "kbps": 3000});
    }
    v
}

#[test]
fn a_programme_whose_picture_comes_after_its_sound_keeps_mpeg2_carried_and_is_planned_when_it_comes() {
    let st = station();
    assert_eq!(row(&st)["carry"], json!({"mpeg2": true, "tracks": true}), "before the input has said anything");

    take_event(&st, "direct.input", &input(false, 0));
    let early = row(&st);
    assert_eq!(early["carry"]["mpeg2"], true, "a rendition refused for want of a picture must not stop the picture coming: {early}");

    take_event(&st, "direct.input", &input(true, 0));
    let planned = row(&st);
    let nodes = planned["transcode"][0]["nodes"].as_array().cloned().unwrap_or_default();
    assert!(nodes.iter().any(|n| n["kind"] == "decode" && n["codec"] == "mpeg2"), "the picture, once there, is planned: {planned}");
    assert!(planned["outputs"].as_array().is_some_and(|o| o.iter().any(|o| o["id"] == "flussonic" && o["rendition"] == true)), "{planned}");

    take_event(&st, "direct.input", &input(true, 1));
    let both = row(&st);
    let encodes = both["transcode"][0]["nodes"].as_array().map_or(0, |n| n.iter().filter(|n| n["kind"] == "aencode").count());
    assert_eq!(encodes, 2, "a second sound track coming later is planned too: {both}");
}
