//! A stream says when its bit rates mean something. The core plans a
//! converting destination only after that: planned on a rate of 0, a
//! destination that asked for 128 kbit/s sound was copied the programme's 160.

use super::*;
use crate::media_tag::{MediaTag, TagKind};
use std::time::Duration;

fn audio(header: bool, size: usize) -> MediaTag {
    let mut body = vec![0u8; size.max(4)];
    body[0] = 0xaf;
    body[1] = if header { 0 } else { 1 };
    MediaTag { kind: TagKind::Audio, timestamp_ms: 0, keyframe: false, sequence_header: header, payload: Arc::from(body) }
}

#[test]
fn a_stream_says_once_its_rates_are_measured_and_the_first_rates_are_news() {
    let hub = Hub::new();
    let p = hub.publish("church", "radio", "x", None).unwrap();
    p.push(audio(true, 4));
    p.push(audio(false, 400));
    let first = hub.stream("church", "radio").unwrap();
    assert_eq!(first["measured"], false, "{first}");
    assert_eq!(first["audio"]["kbps"], 0, "{first}");
    std::thread::sleep(Duration::from_millis(1050));
    // A stream with no video never has a frame rate to move, so the first
    // rates have to be news by themselves for the core to hear of them.
    assert!(p.push(audio(false, 400)), "the first measured rates are worth an event");
    let measured = hub.stream("church", "radio").unwrap();
    assert_eq!(measured["measured"], true, "{measured}");
    assert!(measured["audio"]["kbps"].as_u64().unwrap() > 0, "{measured}");
    assert!(!p.push(audio(false, 400)), "and only once");
}
