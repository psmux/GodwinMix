//! What an output request and an input may say, without a station.

use super::edit::{add, set};
use crate::station::shows_direct::check_input;
use godwinmix_protocol::shows::{InputSpec, ShowOutputSetRequest, ShowOutputSpec};
use serde_json::json;

fn spec(v: serde_json::Value) -> ShowOutputSpec {
    serde_json::from_value(v).unwrap()
}

fn input(uri: &str) -> InputSpec {
    InputSpec { uri: uri.into(), program: None, params: None, backup: None }
}

#[test]
fn an_output_takes_its_platform_from_its_scheme_and_keeps_an_id_it_was_given() {
    let mut list = Vec::new();
    assert_eq!(add(&mut list, "feed", &spec(json!({"uri": "srt://10.0.0.9:9000"}))).unwrap(), "srt");
    assert_eq!(add(&mut list, "feed", &spec(json!({"uri": "udp://239.2.2.2:5000"}))).unwrap(), "udp");
    assert_eq!(add(&mut list, "feed", &spec(json!({"uri": "udp://239.2.2.3:5000"}))).unwrap(), "udp-2");
    assert_eq!(add(&mut list, "feed", &spec(json!({"id": "cdn", "uri": "rtmp://h/app/key"}))).unwrap(), "cdn");
    assert_eq!(list.iter().map(|d| d.platform.as_str()).collect::<Vec<_>>(), ["srt", "udp", "udp", "custom"]);
    assert!(list.iter().all(|d| d.stream == "main"));
    let twice = add(&mut list, "feed", &spec(json!({"id": "cdn", "uri": "rtmp://h/app/k2"}))).unwrap_err();
    assert_eq!(twice.data["field"], "id", "{twice:?}");
    let bad = add(&mut list, "feed", &spec(json!({"id": "Not A Slug", "uri": "srt://h:1"}))).unwrap_err();
    assert_eq!(bad.data["field"], "id");
}

#[test]
fn a_platform_needs_its_key_and_a_plain_output_keeps_its_scheme() {
    let mut list = Vec::new();
    let no_key = add(&mut list, "feed", &spec(json!({"platform": "youtube"}))).unwrap_err();
    assert_eq!(no_key.data["field"], "key");
    let wrong = add(&mut list, "feed", &spec(json!({"platform": "udp", "uri": "srt://h:1"}))).unwrap_err();
    assert_eq!(wrong.data["field"], "uri");
    add(&mut list, "feed", &spec(json!({"uri": "rtp://239.3.3.3:5004"}))).unwrap();
    let req = |v| serde_json::from_value::<ShowOutputSetRequest>(v).unwrap();
    set(&mut list, &req(json!({"id": "feed", "output": "rtp", "enabled": false}))).unwrap();
    assert!(!list[0].enabled);
    let moved = set(&mut list, &req(json!({"id": "feed", "output": "rtp", "uri": "udp://h:1"}))).unwrap_err();
    assert_eq!(moved.data["field"], "uri");
    let missing = set(&mut list, &req(json!({"id": "feed", "output": "nope"}))).unwrap_err();
    assert_eq!(missing.data["kind"], "output");
    set(&mut list, &req(json!({"id": "feed", "output": "rtp", "rendition": {"preset": "youtube-720p30"}}))).unwrap();
    assert!(list[0].rendition.is_some());
    set(&mut list, &req(json!({"id": "feed", "output": "rtp", "rendition": null}))).unwrap();
    assert!(list[0].rendition.is_none(), "null goes back to a copy");
}

#[test]
fn an_input_is_an_address_this_machine_can_open_or_a_channels_stream() {
    for ok in ["udp://@239.1.1.1:5000", "srt://h:9000?mode=caller", "https://h/x.m3u8", "file:///clip.ts", "channel:church/main"] {
        assert!(check_input(&input(ok)).is_ok(), "{ok}");
    }
    for no in ["nonsense", "ftp://h/x", "channel:church", "udp://"] {
        let e = check_input(&input(no)).unwrap_err();
        assert_eq!(e.data["field"], "input.uri", "{no}");
    }
    let mut with_backup = input("udp://@239.1.1.1:5000");
    with_backup.backup = Some(godwinmix_protocol::shows::BackupInput { uri: "nonsense".into(), program: None, params: None });
    assert_eq!(check_input(&with_backup).unwrap_err().data["field"], "input.backup.uri");
}

#[test]
fn an_hls_output_is_named_by_its_address_and_keeps_its_params_on_it() {
    let mut list = Vec::new();
    let made = add(&mut list, "feed", &spec(json!({"uri": "hls://viewers", "params": {"segment_ms": 1000, "window": 6}}))).unwrap();
    assert_eq!((made.as_str(), list[0].platform.as_str()), ("viewers", "hls"));
    assert_eq!(list[0].server, "hls://viewers?segment_ms=1000&window=6");
    let not_hls = add(&mut list, "feed", &spec(json!({"uri": "udp://h:1", "params": {"window": 6}}))).unwrap_err();
    assert_eq!(not_hls.data["field"], "params");
    let ladder = add(&mut list, "feed", &spec(json!({"uri": "hls://abr", "rendition": {"preset": "abr-ladder-4"}}))).unwrap_err();
    assert_eq!(ladder.data["ladder"], true);
    let short = add(&mut list, "feed", &spec(json!({"uri": "hls://x", "params": {"segment_ms": 100}}))).unwrap_err();
    assert_eq!(short.data["field"], "params");
    let req = |v| serde_json::from_value::<ShowOutputSetRequest>(v).unwrap();
    set(&mut list, &req(json!({"id": "feed", "output": "viewers", "label": "Lobby"}))).unwrap();
    assert_eq!(list[0].server, "hls://viewers?segment_ms=1000&window=6", "a label leaves the params be");
    set(&mut list, &req(json!({"id": "feed", "output": "viewers", "params": {"low_latency": true}}))).unwrap();
    assert_eq!(list[0].server, "hls://viewers?low_latency=true", "params given replace them all");
    let moved = set(&mut list, &req(json!({"id": "feed", "output": "viewers", "uri": "udp://h:1"}))).unwrap_err();
    assert_eq!(moved.data["field"], "uri");
}
