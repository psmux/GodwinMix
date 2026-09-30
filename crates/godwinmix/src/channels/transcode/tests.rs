//! Channel renditions planned and admitted, with a real planner and a real
//! governor, and a machine stated rather than probed.

use std::time::{Duration, Instant};

use godwinmix_core::catalogue::select::FakeRegistry;
use godwinmix_core::catalogue::Catalogue;
use godwinmix_govern::load::Load;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::destination::{DestinationMode, StoredDestination};
use godwinmix_protocol::rendition::RenditionChoice;
use serde_json::{json, Value};

use super::outcome::Outcome;
use super::{Machine, Transcode};
use crate::channels::{Live, Record};

fn machine(hardware: bool) -> Machine {
    let mut names = vec!["x264enc", "x265enc", "avdec_h264", "avdec_h265", "h264parse", "h265parse", "avenc_aac", "avdec_aac", "aacparse"];
    if hardware {
        names.extend(["vtenc_h264_hw", "vtdec_hw"]);
    }
    Machine::probe(&Catalogue::shipped().unwrap(), &FakeRegistry::with(&names))
}

fn governor(cores: u32) -> Governor {
    Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), cores, 16_384)
}

fn record() -> Record {
    toml::from_str("id = \"church\"\nname = \"Church\"\napp = \"church\"\n").unwrap()
}

fn dest(id: &str, ask: Option<Value>) -> StoredDestination {
    StoredDestination {
        id: id.into(),
        platform: "custom".into(),
        label: id.into(),
        server: format!("rtmp://127.0.0.1:19392/live/{id}"),
        key: None,
        stream: "*".into(),
        enabled: true,
        rendition: ask.map(|a| serde_json::from_value::<RenditionChoice>(a).unwrap()),
    }
}

fn live(width: u32, height: u32, fps: f64, kbps: u32) -> Live {
    Live::from_plugin(&json!({
        "channel": "church", "app": "church", "stream": "main", "since_ms": 1,
        "video": {"codec": "h264", "width": width, "height": height, "fps": fps, "kbps": kbps, "frame_rate": fps},
        "audio": {"codec": "aac", "channels": 2, "sample_rate": 48000, "kbps": 128},
    }))
    .map(|mut l| {
        l.channel = "church".into();
        l
    })
    .unwrap()
}

fn replan(t: &Transcode, dests: &[StoredDestination], streams: &[Live]) {
    t.replan(&[record()], streams, |_| dests.to_vec());
}

fn base(d: &StoredDestination) -> Value {
    json!({"id": d.id, "platform": d.platform, "url": d.url(), "stream": d.stream})
}

fn count(t: &Transcode, kind: &str) -> usize {
    t.streams("church").map_or(0, |s| s[0]["nodes"].as_array().unwrap().iter().filter(|n| n["kind"] == kind).count())
}

fn ticket(t: &Transcode, node: &str) -> Option<u64> {
    t.state.lock().channels.get("church")?.held.get(node).map(|h| h.ticket.id())
}

#[test]
fn three_destinations_wanting_720p_share_one_decode_and_one_encoder_and_the_source_is_a_copy() {
    let t = Transcode::with(machine(true), governor(8));
    let yt = json!({"preset": "youtube-720p30"});
    let dests = vec![dest("a", Some(yt.clone())), dest("b", Some(yt.clone())), dest("c", Some(yt)), dest("same", Some(json!({"video": {"height": 1080}}))), dest("plain", None)];
    replan(&t, &dests, &[live(1920, 1080, 30.0, 6000)]);
    assert_eq!((count(&t, "decode"), count(&t, "scale"), count(&t, "encode")), (1, 1, 1), "{:#}", t.streams("church").unwrap());
    let nodes = t.streams("church").unwrap();
    let encode = nodes[0]["nodes"].as_array().unwrap().iter().find(|n| n["kind"] == "encode").unwrap().clone();
    assert_eq!(encode["element"], "vtenc_h264_hw", "hardware first: {encode:#}");
    assert_eq!(encode["props"]["bitrate"], 3000);
    assert_eq!(encode["props"]["max-keyframe-interval"], 60, "two seconds at 30 fps");
    let decode = nodes[0]["nodes"].as_array().unwrap().iter().find(|n| n["kind"] == "decode").unwrap().clone();
    assert_eq!(decode["element"], "vtdec_hw");
    for id in ["a", "b", "c"] {
        let row = t.row("church", &dests.iter().find(|d| d.id == id).unwrap().clone(), base(&dests[0])).unwrap();
        assert_eq!(row["video"], encode["id"], "{row}");
        assert_eq!(row["audio"], "copy:main:audio", "the sound already is AAC at 128k: {row}");
    }
    for d in &dests[3..] {
        assert_eq!(t.row("church", d, base(d)), Some(base(d)), "a copy is the row it always was");
    }
    assert_eq!(t.view("church", "same").0.unwrap().mode, DestinationMode::Copy);
    let (plan, _) = t.view("church", "a");
    let plan = plan.unwrap();
    assert_eq!(plan.encoder.as_deref(), Some("h264-videotoolbox"));
    assert!(plan.reason.contains("1920x1080"), "{}", plan.reason);
}

#[test]
fn with_the_hardware_hidden_the_plan_uses_x264_and_the_software_decoder() {
    let t = Transcode::with(machine(false), governor(8));
    replan(&t, &[dest("a", Some(json!({"preset": "youtube-720p30"})))], &[live(1920, 1080, 30.0, 6000)]);
    let nodes = t.streams("church").unwrap()[0]["nodes"].clone();
    let element = |kind: &str| nodes.as_array().unwrap().iter().find(|n| n["kind"] == kind).unwrap()["element"].clone();
    assert_eq!(element("encode"), "x264enc");
    assert_eq!(element("decode"), "avdec_h264");
}

#[test]
fn a_destination_waits_for_its_stream_and_never_goes_out_as_a_copy_meanwhile() {
    let t = Transcode::with(machine(true), governor(8));
    let d = dest("a", Some(json!({"preset": "youtube-720p30"})));
    replan(&t, std::slice::from_ref(&d), &[]);
    let row = t.row("church", &d, base(&d)).unwrap();
    assert_eq!(row["rendition"], true);
    assert!(row.get("video").is_none(), "{row}");
    assert!(t.streams("church").is_none());
    assert!(ticket(&t, "decode:main:video").is_none(), "nothing is admitted for a stream that is not there");
}

#[test]
fn a_publisher_changing_size_moves_only_the_nodes_that_changed() {
    let t = Transcode::with(machine(false), governor(16));
    let dests = vec![dest("hd", Some(json!({"preset": "youtube-720p30"}))), dest("sd", Some(json!({"video": {"height": 480, "bitrate_kbps": 1400}})))];
    replan(&t, &dests, &[live(1920, 1080, 30.0, 6000)]);
    let sd_scale = "scale:main:854x480p30";
    let sd_encode = "encode:main:h264:854x480p30:1400k:g2000";
    let before = (ticket(&t, "decode:main:video").unwrap(), ticket(&t, sd_scale).unwrap(), ticket(&t, sd_encode).unwrap());
    assert!(ticket(&t, "scale:main:1280x720p30").is_some());
    replan(&t, &dests, &[live(1280, 720, 30.0, 6000)]);
    assert!(ticket(&t, "scale:main:1280x720p30").is_none(), "720p needs no scale from a 720p source");
    let after = (ticket(&t, "decode:main:video").unwrap(), ticket(&t, sd_scale).unwrap(), ticket(&t, sd_encode).unwrap());
    assert_eq!(before, after, "the decode, the 480p scale and its encoder keep running");
    let hd = ticket(&t, "encode:main:h264:1280x720p30:3000k:g2000").unwrap();
    replan(&t, &dests, &[live(1280, 720, 30.0, 5200)]);
    assert_eq!(ticket(&t, "encode:main:h264:1280x720p30:3000k:g2000"), Some(hd), "a new bit rate alone moves nothing");
}

#[test]
fn the_governor_refuses_what_a_small_cpu_cannot_carry_and_says_what_fits() {
    let t = Transcode::with(machine(false), governor(6));
    let want = |kbps: u32| Some(json!({"video": {"codec": "h264", "height": 1080, "fps": {"num": 60, "den": 1}, "bitrate_kbps": kbps}}));
    let dests = vec![dest("one", want(6000)), dest("two", want(4500))];
    replan(&t, &dests, &[live(1920, 1080, 30.0, 6000)]);
    let (_, no) = t.view("church", "two");
    let no = no.expect("six cores cannot carry two 1080p60 x264 encodes");
    assert_eq!(no.code, "governor");
    assert!(no.message.contains("needs"), "{}", no.message);
    assert!(!no.advice.is_empty(), "something smaller is offered: {no:?}");
    assert!(no.advice[0].request.video.as_ref().unwrap().height.unwrap() < 1080 || no.advice[0].request.video.as_ref().unwrap().fps.unwrap().num == 30);
    assert_eq!(t.row("church", &dests[1], base(&dests[1])), None, "a refused destination is not handed to the listener");
    let e = t.refusal_error("church", &dests[1]).expect("the edit that asked for it is answered with the refusal");
    assert_eq!(e.code, godwinmix_protocol::error::ErrorCode::Safety.number());
    assert!(e.data["need"]["cpu_millicores"].as_u64().unwrap() > e.data["have"]["cpu_millicores"].as_u64().unwrap());
    assert!(e.data["advice"][0]["request"]["video"]["height"].is_number(), "{}", e.data);
    assert!(t.refusal_error("church", &dests[0]).is_none());
    assert!(matches!(t.state.lock().outcome("church", "one"), Some(Outcome::Transcode { .. })));
}

#[test]
fn a_shed_destination_says_why_and_comes_back_when_there_is_room() {
    let gov = governor(8);
    let t = Transcode::with(machine(false), gov.clone());
    let dests = vec![dest("a", Some(json!({"preset": "youtube-720p30"})))];
    let streams = [live(1920, 1080, 30.0, 6000)];
    replan(&t, &dests, &streams);
    assert!(t.busy());
    gov.load_cell().store(&Load { system_millicores: 7_900, own_millicores: 100, others_peak_millicores: 2_000, samples: 3, ..Load::default() });
    let now = Instant::now();
    let tick = t.tick_at(now);
    assert!(tick.replan && !tick.alerts.is_empty(), "{tick:?}");
    assert!(tick.alerts[0].contains("comes back when there is room"), "{}", tick.alerts[0]);
    replan(&t, &dests, &streams);
    let (_, no) = t.view("church", "a");
    assert_eq!(no.map(|n| n.code).as_deref(), Some("shed"));
    assert_eq!(t.row("church", &dests[0], base(&dests[0])), None);

    gov.load_cell().store(&Load { system_millicores: 1_000, own_millicores: 100, others_peak_millicores: 500, samples: 4, ..Load::default() });
    assert!(!t.tick_at(now + Duration::from_secs(5)).replan, "not straight back");
    assert!(t.tick_at(now + Duration::from_secs(31)).replan);
    replan(&t, &dests, &streams);
    assert!(t.view("church", "a").0.is_some(), "back on");
}

#[path = "tests_hevc.rs"]
mod hevc;
