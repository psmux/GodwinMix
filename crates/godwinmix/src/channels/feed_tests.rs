//! A channel's feed on a real registry over a real channels file and secret
//! store: the default, the programme's key and address, a pull's sealed
//! address, and each taken back when the feed changes.

use godwinmix_protocol::channel_feed::{FeedKind, FeedRequest, FeedState};
use godwinmix_protocol::channels::{ChannelKeyRemoveRequest, ChannelSetRequest};
use serde_json::json;

use super::super::newkey::typed_tests::{add, open, scratch};
use super::output_id;

fn set(id: &str, feed: serde_json::Value) -> ChannelSetRequest {
    ChannelSetRequest { id: id.into(), feed: Some(serde_json::from_value(feed).unwrap()), ..Default::default() }
}

#[tokio::test]
async fn a_channel_is_fed_by_an_encoder_until_told_otherwise() {
    let dir = scratch("feed-default");
    let channels = open(&dir);
    let made = channels.add(add(json!({"name": "Hall"}))).unwrap().channel;
    assert_eq!(made.feed.kind, FeedKind::Encoder);
    assert_eq!(made.feed.state, FeedState::Waiting);
    assert!(channels.programme_feeds().is_empty());
    let file = std::fs::read_to_string(dir.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(!file.contains("feed"), "an encoder is written as before feeds: {file}");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn the_programme_publishes_with_a_key_of_its_own_and_gives_it_back() {
    let dir = scratch("feed-programme");
    let channels = open(&dir);
    let made = channels.add(add(json!({"name": "Live", "feed": {"kind": "programme"}}))).unwrap().channel;
    assert_eq!(made.feed.kind, FeedKind::Programme);
    assert_eq!(made.feed.output.as_deref(), Some(output_id("live").as_str()));
    let key = made.keys.iter().find(|k| k.label == "Programme").expect("a key made for the programme");
    let feeds = channels.programme_feeds();
    assert_eq!(feeds.len(), 1);
    assert_eq!(feeds[0].output, "to-channel-live");
    assert!(feeds[0].uri.starts_with("rtmp://127.0.0.1:") && feeds[0].uri.contains("/live/programme?psk="), "{}", feeds[0].uri);
    assert!(feeds[0].uri.ends_with(&key.hint), "the programme's own key: {}", feeds[0].uri);

    // Choosing it again keeps the key; a removed one is made again.
    channels.set(set("live", json!({"kind": "programme", "show": "second"}))).unwrap();
    assert_eq!(channels.programme_feeds()[0].show.as_deref(), Some("second"));
    assert_eq!(channels.get("live").unwrap().keys.len(), 2, "no second programme key");
    channels.key_remove(ChannelKeyRemoveRequest { id: "live".into(), key: key.id.clone() }).unwrap();
    assert!(channels.programme_feeds().is_empty(), "no key, nothing to publish with");
    channels.set(set("live", json!({"kind": "programme"}))).unwrap();
    assert_eq!(channels.programme_feeds().len(), 1);

    let back = channels.set(set("live", json!({"kind": "encoder"}))).unwrap();
    assert_eq!(back.feed.kind, FeedKind::Encoder);
    assert!(back.keys.iter().all(|k| k.label != "Programme"), "the programme's key goes with it");
    assert!(channels.programme_feeds().is_empty());
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn the_programme_needs_rtmp_and_says_how_to_switch_it_on() {
    let dir = scratch("feed-rtmp");
    let channels = open(&dir);
    channels.add(add(json!({"name": "Srt only", "protocols": ["srt"]}))).unwrap();
    let err = channels.set(set("srt-only", json!({"kind": "programme"}))).unwrap_err();
    assert_eq!(err.data["field"], "feed");
    assert!(err.message.contains("Switch RTMP on"), "{}", err.message);
    let err = channels.add(add(json!({"name": "Other", "protocols": ["whip"], "feed": {"kind": "programme"}}))).unwrap_err();
    assert!(err.message.contains("RTMP"), "{}", err.message);
    assert!(channels.get("other").is_err(), "a refused feed leaves no channel half made");
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn a_pull_address_is_sealed_never_shown_and_handed_to_the_listener() {
    let dir = scratch("feed-pull");
    let channels = open(&dir);
    channels.add(add(json!({"name": "Relay"}))).unwrap();
    let url = "rtmp://user:pw@cdn.example:1935/live/sekret-key";
    let got = channels.set(set("relay", json!({"kind": "pull", "url": url}))).unwrap();
    assert_eq!(got.feed.kind, FeedKind::Pull);
    assert_eq!(got.feed.uri_host.as_deref(), Some("rtmp://cdn.example:1935"));
    assert_eq!(got.feed.state, FeedState::Connecting);
    let shown = serde_json::to_string(&channels.list()).unwrap();
    assert!(!shown.contains("sekret") && !shown.contains("pw@"), "never echoed: {shown}");
    let file = std::fs::read_to_string(dir.join("godwinmix.runtime.channels.toml")).unwrap();
    assert!(!file.contains("sekret"), "sealed, not in the channels file: {file}");
    let record = channels.records.lock().iter().find(|r| r.id == "relay").cloned().unwrap();
    assert_eq!(channels.pull_url(&record).as_deref(), Some(url));

    channels.pull_report(&json!({"channel": "relay", "state": "retrying", "error": "connection refused", "retry_in_ms": 4000}));
    let feed = channels.get("relay").unwrap().feed;
    assert_eq!(feed.state, FeedState::Reconnecting);
    assert_eq!(feed.error.as_deref(), Some("connection refused"));
    assert!(feed.retry_in_ms.is_some_and(|ms| ms <= 4000 && ms > 3000), "{feed:?}");

    let err = channels.set(set("relay", json!({"kind": "pull", "url": "udp://239.1.1.1:5000"}))).unwrap_err();
    assert_eq!(err.data["field"], "feed.url");
    channels.set(set("relay", json!({"kind": "encoder"}))).unwrap();
    assert_eq!(channels.pull_url(&channels.records.lock().iter().find(|r| r.id == "relay").cloned().unwrap()), None);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn a_request_without_a_feed_moves_nothing() {
    let req: FeedRequest = serde_json::from_value(json!({"kind": "programme", "show": "main"})).unwrap();
    assert_eq!(req.kind(), FeedKind::Programme);
}
