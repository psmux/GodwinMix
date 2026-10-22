//! A stream says when its bit rates mean something. The core plans a
//! converting destination only after that: planned on a rate of 0, a
//! destination that asked for 128 kbit/s sound was copied the programme's 160.

use super::*;
use crate::media_tag::{MediaTag, TagKind};
use std::time::Duration;

fn tag(kind: TagKind, header: bool, size: usize) -> MediaTag {
    let mut body = vec![0u8; size.max(4)];
    body[0] = if kind == TagKind::Audio { 0xaf } else { 0x17 };
    body[1] = if header { 0 } else { 1 };
    MediaTag { track: 0, kind, timestamp_ms: 0, keyframe: kind == TagKind::Video, sequence_header: header, payload: Arc::from(body) }
}

fn audio(header: bool, size: usize) -> MediaTag {
    tag(TagKind::Audio, header, size)
}

fn second() {
    std::thread::sleep(Duration::from_millis(1050));
}

#[test]
fn a_stream_says_once_its_rates_are_measured_and_the_first_rates_are_news() {
    let hub = Hub::new();
    let p = hub.publish("church", "radio", "x", None).unwrap();
    p.push(audio(true, 4));
    // The programme's sound arrived more than a second after it connected,
    // and that empty second was taken for a measured 0.
    second();
    assert!(!p.push(audio(false, 400)), "a first frame late is not a measured second");
    let first = hub.stream("church", "radio").unwrap();
    assert_eq!(first["measured"], false, "{first}");
    assert_eq!(first["audio"]["kbps"], 0, "{first}");
    second();
    assert!(!p.push(audio(false, 400)), "the first window is a warm up");
    assert_eq!(hub.stream("church", "radio").unwrap()["measured"], false);
    second();
    // A stream with no video never has a frame rate to move, so the first
    // rates have to be news by themselves for the core to hear of them.
    assert!(p.push(audio(false, 400)), "the first measured rates are worth an event");
    let measured = hub.stream("church", "radio").unwrap();
    assert_eq!(measured["measured"], true, "{measured}");
    assert!(measured["audio"]["kbps"].as_u64().unwrap() > 0, "{measured}");
    assert!(!p.push(audio(false, 400)), "and only once");
}

#[test]
fn a_track_that_never_sends_holds_the_rates_back_four_seconds_and_no_longer() {
    let hub = Hub::new();
    let p = hub.publish("church", "silent", "x", None).unwrap();
    p.push(tag(TagKind::Video, true, 30));
    p.push(audio(true, 4));
    p.push(tag(TagKind::Video, false, 20_000));
    for _ in 0..3 {
        second();
        p.push(tag(TagKind::Video, false, 20_000));
        assert_eq!(hub.stream("church", "silent").unwrap()["measured"], false, "sound announced and none sent yet");
    }
    second();
    p.push(tag(TagKind::Video, false, 20_000));
    assert_eq!(hub.stream("church", "silent").unwrap()["measured"], true, "planned at last, with the sound at 0");
}
