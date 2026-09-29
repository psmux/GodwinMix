//! Destinations run from the channel table, against the hub and a real RTMP
//! receiver on loopback.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;

use super::*;
use crate::hub::Publication;
use crate::media_tag::{MediaTag, TagKind};
use crate::restream::test_gate::listen;

fn tag(kind: TagKind, ts: u32, keyframe: bool, header: bool, body: &[u8]) -> MediaTag {
    MediaTag { kind, timestamp_ms: ts, keyframe, sequence_header: header, payload: Arc::from(body) }
}

/// Publish on the hub until `stop`: headers, then a keyframe every ten frames.
fn publish(hub: &Hub, app: &str, stream: &str, stop: Arc<AtomicBool>) -> std::thread::JoinHandle<()> {
    let p: Publication = hub.publish(app, stream, "127.0.0.1:1", Some("k".into())).expect("publish");
    std::thread::spawn(move || {
        p.push(tag(TagKind::Video, 0, true, true, &[0x17, 0x00, 0, 0, 0, 1, 0x64, 0, 0x28]));
        p.push(tag(TagKind::Audio, 0, false, true, &[0xaf, 0x00, 0x12, 0x10]));
        let mut n = 0u32;
        while !stop.load(Ordering::Relaxed) {
            let key = n % 10 == 0;
            let body: &[u8] = if key { &[0x17, 0x01, 0, 0, 0, 0xaa] } else { &[0x27, 0x01, 0, 0, 0, 0xbb] };
            p.push(tag(TagKind::Video, n * 33, key, false, body));
            n += 1;
            std::thread::sleep(Duration::from_millis(33));
        }
    })
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn state(sends: &Sends) -> String {
    sends.rates()[0]["state"].as_str().unwrap_or_default().to_string()
}

#[test]
fn the_table_gives_each_destination_that_is_on_with_its_channel() {
    let params = json!({"channels": [
        {"id": "sunday", "app": "sunday", "enabled": true, "destinations": [
            {"id": "yt", "platform": "youtube", "url": "rtmp://a/live2/k", "stream": "*"},
            {"id": "hall", "platform": "custom", "url": "rtmp://h/app/s"},
            {"id": "off", "platform": "custom", "url": "rtmp://h/app/s", "enabled": false},
        ]},
        {"id": "dark", "app": "dark", "enabled": false, "destinations": [
            {"id": "yt", "platform": "youtube", "url": "rtmp://a/live2/k"},
        ]},
    ]});
    let w = wanted(&params);
    assert_eq!(w.len(), 2);
    assert_eq!((w[0].channel.as_str(), w[0].id.as_str(), w[0].stream.as_str()), ("sunday", "yt", "*"));
    assert_eq!(w[1].stream, "*", "a destination with no stream sends the first live one");
}

#[test]
fn a_destination_waits_sends_while_live_moves_to_the_next_stream_and_stops_when_removed() {
    let got = Arc::new(AtomicUsize::new(0));
    let count = got.clone();
    let receiver = listen(0, None, Arc::new(move |t| {
        if t.is_some() {
            count.fetch_add(1, Ordering::Relaxed);
        }
    }));
    let hub = Hub::new();
    let sends = Sends::new(hub.clone(), None);
    let url = format!("rtmp://127.0.0.1:{}/live/x", receiver.port());
    let w = Wanted {
        channel: "sunday".into(),
        app: "sunday".into(),
        id: "hall".into(),
        platform: "custom".into(),
        url,
        stream: "*".into(),
    };
    sends.apply(vec![w.clone()]);
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(state(&sends), "waiting", "nothing is live yet");

    let (stop_a, stop_b) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicBool::new(false)));
    let a = publish(&hub, "sunday", "main", stop_a.clone());
    wait_for("the destination to go live", || state(&sends) == "live");
    wait_for("tags at the receiver", || got.load(Ordering::Relaxed) > 20);

    let b = publish(&hub, "sunday", "backup", stop_b.clone());
    stop_a.store(true, Ordering::Relaxed);
    a.join().unwrap();
    let before = got.load(Ordering::Relaxed);
    wait_for("the next stream to be sent", || got.load(Ordering::Relaxed) > before + 20);
    assert_eq!(state(&sends), "live");

    sends.apply(vec![]);
    assert!(sends.rates().is_empty());
    stop_b.store(true, Ordering::Relaxed);
    b.join().unwrap();
    assert!(!hub.is_live("sunday", "backup"));
}
