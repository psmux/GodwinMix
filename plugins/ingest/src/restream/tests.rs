//! The restreamer against the plugin's own RTMP listener, over real sockets.

use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use godwinmix_protocol::destination::DestinationState;

use super::*;
use crate::media_tag::TagKind;
use super::test_gate::listen;
use crate::rtmp::Server;

const VIDEO_HEADER: &[u8] = &[0x17, 0x00, 0, 0, 0, 0x01, 0x64, 0x00, 0x28];
const AUDIO_HEADER: &[u8] = &[0xaf, 0x00, 0x12, 0x10];

fn tag(kind: TagKind, ts: u32, keyframe: bool, header: bool, body: &[u8]) -> MediaTag {
    MediaTag { kind, timestamp_ms: ts, keyframe, sequence_header: header, payload: Arc::from(body) }
}

/// Headers, then 30 frames a second with a keyframe every ten, and audio
/// between, until the receiver is dropped.
fn feed(tx: mpsc::Sender<MediaTag>) {
    let meta = rml_amf0::serialize(&vec![
        rml_amf0::Amf0Value::Utf8String("onMetaData".into()),
        rml_amf0::Amf0Value::Object([("width".to_string(), rml_amf0::Amf0Value::Number(64.0))].into()),
    ])
    .unwrap();
    let _ = tx.send(tag(TagKind::Script, 0, false, false, &meta));
    let _ = tx.send(tag(TagKind::Video, 0, true, true, VIDEO_HEADER));
    let _ = tx.send(tag(TagKind::Audio, 0, false, true, AUDIO_HEADER));
    std::thread::spawn(move || {
        for n in 0u32.. {
            let ts = 5_000 + n * 33;
            let key = n % 10 == 0;
            let body: &[u8] = if key { &[0x17, 0x01, 0, 0, 0, 0xaa] } else { &[0x27, 0x01, 0, 0, 0, 0xbb] };
            if tx.send(tag(TagKind::Video, ts, key, false, body)).is_err()
                || tx.send(tag(TagKind::Audio, ts, false, false, &[0xaf, 0x01, 0xcc])).is_err()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(33));
        }
    });
}

/// A listener that keeps every tag a publisher sent it.
fn receiver(port: u16) -> (Server, Arc<Mutex<Vec<MediaTag>>>) {
    let got = Arc::new(Mutex::new(Vec::new()));
    let keep = Arc::clone(&got);
    let server = listen(port, None, Arc::new(move |t| {
        if let Some(t) = t {
            keep.lock().unwrap().push(t);
        }
    }));
    (server, got)
}

/// The tags as (FLV tag type, body).
fn tags(got: &[MediaTag]) -> Vec<(u8, Vec<u8>)> {
    got.iter()
        .map(|t| {
            let kind = match t.kind {
                TagKind::Video => 9,
                TagKind::Audio => 8,
                TagKind::Script => 18,
            };
            (kind, t.payload.to_vec())
        })
        .collect()
}

fn wait_for(what: &str, mut ok: impl FnMut() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !ok() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn a_receiver_that_comes_back_gets_the_headers_again_and_then_a_keyframe() {
    let (first, got) = receiver(0);
    let port = first.port();
    let (tx, rx) = mpsc::channel();
    feed(tx);
    let url = format!("rtmp://127.0.0.1:{port}/live/fake-youtube");
    let dest = start(Target::new("fake", "custom", &url), rx);
    wait_for("the first connection", || tags(&got.lock().unwrap()).len() > 20);
    assert_eq!(dest.stats().live.state, DestinationState::Live);

    drop(first);
    let (_second, again) = receiver(port);
    wait_for("the reconnect", || tags(&again.lock().unwrap()).len() > 20);

    let after = tags(&again.lock().unwrap());
    assert_eq!(after[0].0, 18, "the metadata comes first");
    assert_eq!(after[1], (9, VIDEO_HEADER.to_vec()), "then the AVC header");
    assert_eq!(after[2], (8, AUDIO_HEADER.to_vec()), "then the AAC header");
    assert_eq!(after[3].1[..2], [0x17, 0x01], "then a keyframe, never an inter frame");
    let stats = dest.stats();
    assert_eq!(stats.live.state, DestinationState::Live);
    assert_eq!(stats.live.reconnects, 1);
    assert_eq!(stats.dropped.gops, 0, "a quick reconnect loses nothing to the queue");
}

#[test]
fn a_wrong_key_is_failed_with_a_sentence_that_says_so() {
    let server = listen(0, Some("the-right-key"), Arc::new(|_| {}));
    let (tx, rx) = mpsc::channel();
    feed(tx);
    let url = format!("rtmp://127.0.0.1:{}/live/a-wrong-key", server.port());
    let dest = start(Target::new("yt", "custom", &url), rx);
    wait_for("the refusal", || dest.stats().live.state == DestinationState::Failed);
    let error = dest.stats().live.error.unwrap();
    assert!(error.contains("refused the key"), "{error}");
    assert!(!error.contains("a-wrong-key"), "the key must not reach a log: {error}");
}

#[test]
fn nothing_listening_is_said_in_plain_words_and_retried() {
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let (tx, rx) = mpsc::channel();
    feed(tx);
    let dest = start(Target::new("gone", "custom", &format!("rtmp://127.0.0.1:{port}/live/k")), rx);
    wait_for("the first failure", || dest.stats().live.error.is_some());
    let stats = dest.stats();
    assert_eq!(stats.live.state, DestinationState::Connecting);
    assert_eq!(stats.live.error.unwrap(), format!("nothing answered at rtmp://127.0.0.1:{port}"));
    dest.stop();
    assert_eq!(dest.stats().live.state, DestinationState::Off);
}
