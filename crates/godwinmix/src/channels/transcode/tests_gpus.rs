//! Channels on a machine with two NVIDIA cards: a card named, `auto`
//! spreading them by load, a placed channel staying put, and a rebalance
//! that moves the fewest. The machine is stated, as GStreamer names the
//! second card's elements (`nvh264device1enc`).

use std::collections::BTreeMap;

use godwinmix_core::catalogue::select::FakeRegistry;
use godwinmix_core::catalogue::Catalogue;
use godwinmix_core::config::GpuSpread;
use godwinmix_govern::{Governor, GovernorConfig, Profile};
use godwinmix_protocol::destination::StoredDestination;
use godwinmix_protocol::rendition::RenditionChoice;
use serde_json::{json, Value};

use super::{Machine, Transcode};
use crate::channels::{Live, Record};

const ONE: &[&str] = &["nvh264enc", "nvh264dec", "nvmpeg2videodec", "cudaconvertscale", "cudadownload", "h264parse", "x264enc", "avdec_h264", "avdec_mpeg2video", "a52dec", "ac3parse", "avenc_aac", "avdec_aac", "aacparse"];

fn machine(cards: u32) -> Machine {
    let mut names: Vec<String> = ONE.iter().map(|s| s.to_string()).collect();
    for d in 1..cards {
        names.extend([format!("nvh264device{d}enc"), format!("nvh264device{d}dec"), format!("nvmpeg2videodevice{d}dec")]);
    }
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    Machine::probe(&Catalogue::shipped().unwrap(), &FakeRegistry::with(&names))
}

fn station(cards: u32) -> Transcode {
    Transcode::with(machine(cards), Governor::with_machine(GovernorConfig::default(), Profile::uncalibrated(), 48, 64 * 1024))
}

fn record(id: &str) -> Record {
    toml::from_str(&format!("id = \"{id}\"\nname = \"{id}\"\napp = \"{id}\"\n")).unwrap()
}

/// An imported transcoder line's request, with `gpu` as given.
fn channel(id: &str, gpu: Option<Value>) -> StoredDestination {
    let mut ask = json!({"video": {"codec": "h264", "width": 1280, "height": 720, "bitrate_kbps": 1500, "keyframe_frames": 50,
                                   "fit": "pad", "pad_color": "#000000", "deinterlace": "off", "preset": "p1"},
                         "audio": {"codec": "aac", "bitrate_kbps": 128}});
    if let Some(g) = gpu {
        ask["video"]["gpu"] = g;
    }
    StoredDestination {
        id: format!("{id}-hls"),
        platform: "hls".into(),
        label: id.into(),
        server: String::new(),
        key: None,
        stream: "*".into(),
        enabled: true,
        rendition: Some(serde_json::from_value::<RenditionChoice>(ask).unwrap()),
    }
}

fn programme(id: &str) -> Live {
    let mut l = Live::from_plugin(&json!({
        "channel": id, "app": id, "stream": "main", "since_ms": 1,
        "video": {"codec": "mpeg2", "width": 1920, "height": 1080, "fps": 29.97, "kbps": 8000, "frame_rate": 29.97, "interlaced": true},
        "audio": {"codec": "ac3", "channels": 2, "sample_rate": 48000, "kbps": 192},
    }))
    .unwrap();
    l.channel = id.into();
    l
}

fn ids(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("ch{i:02}")).collect()
}

/// Plan `ids`, each asking for `gpu`, every one live.
fn plan(t: &Transcode, ids: &[String], gpu: impl Fn(&str) -> Option<Value>) {
    let records: Vec<Record> = ids.iter().map(|id| record(id)).collect();
    let lives: Vec<Live> = ids.iter().map(|id| programme(id)).collect();
    t.replan(&records, &lives, |r| vec![channel(&r.id, gpu(&r.id))]);
}

fn nodes(t: &Transcode, id: &str) -> Vec<Value> {
    t.streams(id).unwrap_or_else(|| panic!("{id} has no plan"))[0]["nodes"].as_array().unwrap().clone()
}

fn count(placed: &BTreeMap<String, u32>, card: u32) -> usize {
    placed.values().filter(|c| **c == card).count()
}

#[test]
fn a_channel_set_on_gpu_1_decodes_scales_and_encodes_there() {
    let t = station(2);
    plan(&t, &ids(1), |_| Some(json!(1)));
    let nodes = nodes(&t, "ch00");
    let of = |kind: &str| nodes.iter().find(|n| n["kind"] == kind && n["track"] != "audio").cloned().unwrap();
    assert_eq!(of("decode")["element"], "nvmpeg2videodevice1dec");
    assert_eq!(of("encode")["element"], "nvh264device1enc");
    assert_eq!(of("scale")["element"], "cudaconvertscale");
    for kind in ["decode", "scale", "encode"] {
        assert_eq!(of(kind)["gpu"], 1, "{kind} sets its card for the listener");
        assert_eq!(of(kind)["memory"], "cuda", "{kind} stays in that card's memory");
    }
    assert!(nodes.iter().filter(|n| n["track"] == "audio" || n["kind"] == "aencode").all(|n| n.get("gpu").is_none()), "sound is not on a card");
    assert_eq!(t.placements().get("ch00"), Some(&1));
}

#[test]
fn with_one_card_a_plan_is_what_it_always_was() {
    let t = station(1);
    plan(&t, &ids(1), |_| Some(json!("auto")));
    let nodes = nodes(&t, "ch00");
    assert!(nodes.iter().all(|n| n.get("gpu").is_none()), "{nodes:?}");
    assert!(nodes.iter().any(|n| n["element"] == "nvh264enc"));
    assert!(nodes.iter().any(|n| n["element"] == "nvmpeg2videodec"));
}

#[test]
fn auto_deals_channels_out_evenly_and_each_card_holds_its_own_sessions() {
    let t = station(2);
    plan(&t, &ids(10), |_| Some(json!("auto")));
    let placed = t.placements();
    assert_eq!((count(&placed, 0), count(&placed, 1)), (5, 5), "{placed:?}");
    for (id, card) in &placed {
        let enc = nodes(&t, id).into_iter().find(|n| n["kind"] == "encode").unwrap();
        assert_eq!(enc["element"], if *card == 1 { "nvh264device1enc" } else { "nvh264enc" }, "{id}");
    }
    let gov = t.governor.get();
    let sessions = |d: &str| gov.held().iter().filter(|h| h.device.as_deref() == Some(d)).map(|h| h.cost.device_sessions).sum::<u32>();
    assert_eq!((sessions("nvidia"), sessions("nvidia:1")), (5, 5));
}

#[test]
fn channels_that_start_before_their_input_arrives_are_dealt_out_too() {
    let t = station(2);
    let ids = ids(8);
    let records: Vec<Record> = ids.iter().map(|id| record(id)).collect();
    t.replan(&records, &[], |r| vec![channel(&r.id, Some(json!("auto")))]);
    let placed = t.placements();
    assert_eq!((count(&placed, 0), count(&placed, 1)), (4, 4), "{placed:?}");
}

#[test]
fn a_placed_channel_stays_on_its_card_while_it_runs() {
    let t = station(2);
    plan(&t, &ids(2), |_| Some(json!("auto")));
    let before = t.placements();
    // Four more pinned to card 1 make card 1 the busy one; the first two
    // are not moved for it.
    let more = ids(6);
    plan(&t, &more, |id| if before.contains_key(id) { Some(json!("auto")) } else { Some(json!(1)) });
    let after = t.placements();
    for (id, card) in &before {
        assert_eq!(after.get(id), Some(card), "{id} moved on its own");
    }
}

#[test]
fn spread_places_channels_that_name_no_card_and_first_keeps_them_on_card_0() {
    let t = station(2);
    plan(&t, &ids(4), |_| None);
    assert!(t.placements().is_empty(), "first: no card is chosen, as before");
    assert!(nodes(&t, "ch00").iter().filter(|n| n.get("gpu").is_some()).all(|n| n["gpu"] == 0));
    let t = station(2);
    t.set_gpus(GpuSpread::Spread);
    plan(&t, &ids(4), |_| None);
    let placed = t.placements();
    assert_eq!((count(&placed, 0), count(&placed, 1)), (2, 2), "{placed:?}");
}

#[test]
fn a_card_this_machine_does_not_have_is_taken_as_auto() {
    let t = station(2);
    plan(&t, &ids(2), |_| Some(json!(7)));
    let placed = t.placements();
    assert_eq!((count(&placed, 0), count(&placed, 1)), (1, 1), "{placed:?}");
}

#[test]
fn a_rebalance_moves_the_fewest_and_only_what_auto_placed() {
    let t = station(2);
    let all = ids(6);
    plan(&t, &all, |_| Some(json!(0)));
    let pinned = t.rebalance(None, false);
    assert!(pinned.moved.is_empty(), "a named card is never moved: {pinned:?}");
    // Asked for auto from now on: each stays where it is until a rebalance.
    plan(&t, &all, |_| Some(json!("auto")));
    assert_eq!(count(&t.placements(), 0), 6);
    let dry = t.rebalance(None, true);
    assert_eq!(dry.moved.len(), 3, "{dry:?}");
    assert_eq!(count(&t.placements(), 0), 6, "a dry run moves nothing");
    let done = t.rebalance(None, false);
    assert_eq!(done.moved.len(), 3);
    assert!(done.moved.iter().all(|m| m.from == Some(0) && m.to == 1));
    plan(&t, &all, |_| Some(json!("auto")));
    let placed = t.placements();
    assert_eq!((count(&placed, 0), count(&placed, 1)), (3, 3));
    for m in &done.moved {
        let enc = nodes(&t, &m.id).into_iter().find(|n| n["kind"] == "encode").unwrap();
        assert_eq!(enc["element"], "nvh264device1enc", "{} is planned on its new card", m.id);
    }
    assert!(t.rebalance(None, false).moved.is_empty(), "even cards stay as they are");
}
