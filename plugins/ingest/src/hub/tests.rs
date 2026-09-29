use super::*;
use crate::media_tag::{MediaTag, TagKind};
use std::time::{Duration, Instant};

fn tag(kind: TagKind, ms: u32, keyframe: bool, header: bool, size: usize) -> MediaTag {
    let mut body = vec![0u8; size.max(2)];
    body[0] = match kind {
        TagKind::Video if keyframe => 0x17,
        TagKind::Video => 0x27,
        TagKind::Audio => 0xaf,
        TagKind::Script => 0x02,
    };
    body[1] = if header { 0 } else { 1 };
    MediaTag { kind, timestamp_ms: ms, keyframe, sequence_header: header, payload: Arc::from(body) }
}

/// A publisher's opening: metadata, the two headers.
fn opening(p: &Publication) {
    p.push(tag(TagKind::Script, 0, false, false, 40));
    p.push(tag(TagKind::Video, 0, true, true, 30));
    p.push(tag(TagKind::Audio, 0, false, true, 4));
}

/// One second of 6 Mbit/s at 30 fps with a keyframe every `gop` frames.
fn second(p: &Publication, start: u32, gop: u32) {
    for f in 0..30u32 {
        let ms = start + f * 33;
        let key = (start / 33 + f) % gop == 0;
        p.push(tag(TagKind::Video, ms, key, false, if key { 100_000 } else { 22_000 }));
        p.push(tag(TagKind::Audio, ms, false, false, 400));
    }
}

fn drain(r: &Reader) -> Vec<MediaTag> {
    let mut out = Vec::new();
    while let Recv::Tag(t) = r.recv_timeout(Duration::from_millis(1)) {
        out.push(t);
    }
    out
}

#[test]
fn a_late_joiner_gets_the_metadata_and_both_headers_before_its_first_keyframe() {
    let hub = Hub::new();
    let p = hub.publish("church", "main", "10.0.0.9:5000", Some("obs".into())).unwrap();
    opening(&p);
    second(&p, 0, 60);
    let r = hub.subscribe("church", "main");
    // Mid GOP: inter frames and audio arrive, nothing is queued yet.
    p.push(tag(TagKind::Video, 1000, false, false, 100));
    p.push(tag(TagKind::Audio, 1000, false, false, 100));
    assert!(drain(&r).is_empty(), "a reader never starts on an inter frame");
    p.push(tag(TagKind::Video, 2000, true, false, 100));
    let got = drain(&r);
    let kinds: Vec<(TagKind, bool, bool)> =
        got.iter().map(|t| (t.kind, t.sequence_header, t.keyframe)).collect();
    assert_eq!(
        kinds,
        vec![
            (TagKind::Script, false, false),
            (TagKind::Video, true, true),
            (TagKind::Audio, true, false),
            (TagKind::Video, false, true),
        ]
    );
    // The payload is shared with the publisher's copy, not duplicated.
    assert!(Arc::strong_count(&got[1].payload) >= 2);
}

#[test]
fn a_reader_that_waits_for_a_publisher_gets_it_and_is_told_when_it_leaves() {
    let hub = Hub::new();
    let mut r = hub.subscribe("church", "cam2");
    assert!(!hub.is_live("church", "cam2"));
    let p = hub.publish("church", "cam2", "10.0.0.9:5001", None).unwrap();
    opening(&p);
    second(&p, 0, 30);
    assert!(hub.is_live("church", "cam2"));
    drop(p);
    let got: Vec<MediaTag> = r.by_ref().collect();
    assert_eq!(got[0].kind, TagKind::Script);
    assert_eq!(got.len(), 3 + 60);
    assert!(matches!(r.recv_timeout(Duration::from_millis(1)), Recv::Ended));
    drop(r);
    assert!(hub.streams().is_empty(), "nothing is left behind for a name nobody uses");
}

#[test]
fn a_second_publisher_on_a_live_name_is_refused_and_says_from_where() {
    let hub = Hub::new();
    let _first = hub.publish("church", "main", "10.0.0.9:5000", None).unwrap();
    let err = match hub.publish("church", "main", "10.0.0.7:6000", None) {
        Ok(_) => panic!("two publishers on one name"),
        Err(e) => e,
    };
    assert!(err.contains("10.0.0.9:5000"), "{err}");
}

#[test]
fn a_reader_that_never_reads_loses_whole_gops_and_its_queue_stays_bounded() {
    let hub = Hub::new();
    let p = hub.publish("church", "main", "x", None).unwrap();
    let stuck = hub.subscribe("church", "main");
    opening(&p);
    for s in 0..20 {
        second(&p, s * 1000, 60);
    }
    let (tags, bytes) = stuck.waiting();
    assert!(bytes <= queue::MAX_BYTES && tags <= queue::MAX_TAGS, "{tags} tags, {bytes} bytes");
    assert!(stuck.dropped_gops() > 0);
    // What is left starts at a keyframe, with the headers in front of it.
    let left = drain(&stuck);
    assert_eq!(left[0].kind, TagKind::Script);
    assert!(left[1].sequence_header && left[2].sequence_header);
    assert!(left[3].keyframe && !left[3].sequence_header, "{:?}", left[3]);
    let described = hub.stream("church", "main").unwrap();
    assert!(described["dropped_gops"].as_u64().unwrap() > 0);
}

/// The performance rule: a blocked reader must not slow a publisher, or the
/// reader beside it.
#[test]
fn a_blocked_reader_does_not_slow_the_publisher_or_the_other_reader() {
    let hub = Hub::new();
    let p = hub.publish("church", "main", "x", None).unwrap();
    let blocked = hub.subscribe("church", "main");
    let mut fast = hub.subscribe("church", "main");
    // The blocked reader's thread holds its reader and never reads.
    let (stop_tx, stop_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        let _ = stop_rx.recv();
        blocked.dropped_gops()
    });
    let reading = std::thread::spawn(move || fast.by_ref().count());

    opening(&p);
    let mut elapsed = Duration::ZERO;
    let mut worst = Duration::ZERO;
    for s in 0..60 {
        let t = Instant::now();
        second(&p, s * 1000, 60);
        let took = t.elapsed();
        elapsed += took;
        worst = worst.max(took);
        // A breath between seconds of media, so the reader that keeps up has
        // the time a real one would. Not counted in what the publisher spent.
        std::thread::sleep(Duration::from_millis(5));
    }
    drop(p);
    let delivered = reading.join().unwrap();
    stop_tx.send(()).unwrap();
    let lost = holder.join().unwrap();

    // Sixty seconds of 6 Mbit/s pushed as fast as the loop goes: well under a
    // second on anything, because nothing waits on the stuck reader.
    assert!(elapsed < Duration::from_secs(2), "pushing took {elapsed:?}");
    assert!(worst < Duration::from_millis(200), "one second of media took {worst:?}");
    assert_eq!(delivered, 3 + 60 * 60, "the reader that kept up got every tag");
    assert!(lost > 0, "the stuck reader lost GOPs rather than holding anyone up");
}
