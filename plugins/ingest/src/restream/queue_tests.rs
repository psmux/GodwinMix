use super::*;
use std::sync::Arc;

pub(crate) fn tag(kind: TagKind, ts: u32, keyframe: bool, header: bool, size: usize) -> MediaTag {
    MediaTag {
        kind,
        timestamp_ms: ts,
        keyframe,
        sequence_header: header,
        payload: Arc::from(vec![0u8; size]),
    }
}

fn key(ts: u32) -> MediaTag {
    tag(TagKind::Video, ts, true, false, 100)
}
fn inter(ts: u32) -> MediaTag {
    tag(TagKind::Video, ts, false, false, 100)
}
fn audio(ts: u32) -> MediaTag {
    tag(TagKind::Audio, ts, false, false, 10)
}

fn drain(q: &Queue) -> Vec<MediaTag> {
    let mut out = Vec::new();
    while let Pop::Tag(t) = q.pop(Duration::from_millis(1)) {
        out.push(t);
    }
    out
}

/// One GOP is a keyframe and nine inter frames, 1000 bytes. A queue of 1500
/// holds one and a half. Pushing a second and a third GOP must lose whole GOPs
/// from the front, so what comes out starts at a keyframe, and the headers
/// that came first are still there.
#[test]
fn a_full_queue_drops_whole_gops_from_the_front_and_keeps_the_headers() {
    let q = Queue::new(1500);
    q.push(tag(TagKind::Script, 0, false, false, 20));
    q.push(tag(TagKind::Video, 0, true, true, 30));
    q.push(tag(TagKind::Audio, 0, false, true, 5));
    for gop in 0..3u32 {
        q.push(key(gop * 1000));
        for f in 1..10 {
            q.push(inter(gop * 1000 + f * 33));
        }
    }
    let out = drain(&q);
    assert_eq!(out[0].kind, TagKind::Script);
    assert!(out[1].sequence_header && out[2].sequence_header);
    assert!(starts_gop(&out[3]), "what is left must start at a keyframe");
    assert_eq!(out[3].timestamp_ms, 2000, "the newest GOP is the one kept");
    assert_eq!(out.len(), 3 + 10);
    let d = q.dropped();
    assert_eq!(d.gops, 2);
    assert_eq!(d.tags, 20);
}

/// With no keyframe to stop at, the queue empties and then takes nothing but
/// headers until a keyframe arrives, so a decoder is never handed an inter
/// frame with nothing in front of it.
#[test]
fn with_no_keyframe_left_it_skips_until_the_next_one() {
    let q = Queue::new(250);
    q.push(key(0));
    q.push(inter(33));
    q.push(inter(66)); // 300 bytes, over: no second keyframe, so all go
    assert!(matches!(q.pop(Duration::from_millis(1)), Pop::Empty));
    q.push(inter(100));
    q.push(audio(100));
    q.push(tag(TagKind::Video, 120, true, true, 30)); // a new header gets in
    q.push(key(133));
    q.push(audio(140));
    let out = drain(&q);
    assert!(out[0].sequence_header);
    assert!(starts_gop(&out[1]));
    assert_eq!(out[1].timestamp_ms, 133);
    assert_eq!(out[2].kind, TagKind::Audio);
    assert_eq!(out.len(), 3);
    assert_eq!(q.dropped().tags, 5);
}

#[test]
fn a_destination_that_comes_back_starts_from_the_newest_keyframe() {
    let q = Queue::new(1 << 20);
    q.push(tag(TagKind::Video, 0, true, true, 30));
    q.push(key(0));
    q.push(inter(33));
    q.push(key(1000));
    q.push(audio(1010));
    q.skip_to_latest_keyframe();
    let out = drain(&q);
    assert_eq!(out.len(), 3);
    assert!(out[0].sequence_header);
    assert_eq!(out[1].timestamp_ms, 1000);
    assert_eq!(q.dropped().gops, 0, "a skip on reconnect is not an overflow");
}

#[test]
fn a_closed_queue_still_hands_out_what_it_holds() {
    let q = Queue::new(1 << 20);
    q.push(key(0));
    q.close();
    assert!(matches!(q.pop(Duration::from_millis(1)), Pop::Tag(_)));
    assert!(matches!(q.pop(Duration::from_millis(1)), Pop::Closed));
}

#[test]
fn a_pop_on_an_empty_queue_waits_only_as_long_as_it_was_told() {
    let q = Queue::new(1 << 20);
    let started = std::time::Instant::now();
    assert!(matches!(q.pop(Duration::from_millis(30)), Pop::Empty));
    assert!(started.elapsed() < Duration::from_secs(1));
}
