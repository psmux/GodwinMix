use super::*;
use crate::media_tag::{MediaTag, TagKind};
use crate::hub::Recv;
use crate::rtmp::Kick;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn frame(ms: u32) -> MediaTag {
    let body = vec![0x17, 1, 0, 0, 0];
    MediaTag { kind: TagKind::Video, timestamp_ms: ms, keyframe: true, sequence_header: false, payload: Arc::from(body) }
}

fn kick() -> (Kick, Arc<AtomicBool>) {
    let kicked = Arc::new(AtomicBool::new(false));
    let flag = kicked.clone();
    (Arc::new(move || flag.store(true, Ordering::Relaxed)), kicked)
}

/// Push a frame every 40 ms for `long`.
fn keep_sending(p: &Publication, long: Duration) {
    let end = Instant::now() + long;
    let mut ms = 0;
    while Instant::now() < end {
        p.push(frame(ms));
        ms += 40;
        std::thread::sleep(Duration::from_millis(40));
    }
}

#[test]
fn a_publisher_that_went_quiet_is_cut_off_and_replaced_by_the_one_that_came_back() {
    let hub = Hub::new();
    let (old_kick, kicked) = kick();
    let old = hub.publish_with("church", "main", "tab-1", Some("k".into()), "whip", Some(old_kick)).unwrap();
    old.push(frame(0));
    let reader = hub.subscribe("church", "main");
    assert!(hub.held("church", "main"), "a session that just sent is held");
    std::thread::sleep(STALE + Duration::from_millis(100));
    assert!(!hub.held("church", "main"), "quiet for {STALE:?}, it may be taken over");

    let (new_kick, _) = kick();
    let new = hub.publish_with("church", "main", "tab-2", Some("k".into()), "whip", Some(new_kick)).unwrap();
    assert_eq!(new.took_over(), Some("tab-1"));
    assert!(kicked.load(Ordering::Relaxed), "the old session was cut off with its own kick");
    assert!(!old.current() && new.current());
    let ended = matches!(reader.recv_timeout(Duration::from_millis(10)), Recv::Ended);
    assert!(ended, "its readers were told it ended");

    // What the old one does after that touches nothing.
    assert!(!old.push(frame(40)));
    drop(old);
    assert!(hub.is_live("church", "main"), "the old session's end did not end the new one");
    assert_eq!(hub.stream("church", "main").unwrap()["from"], "tab-2");
}

#[test]
fn a_second_publisher_is_refused_while_the_first_is_still_sending() {
    let hub = Hub::new();
    let (first_kick, kicked) = kick();
    let first = hub.publish_with("church", "main", "tab-1", None, "whip", Some(first_kick)).unwrap();
    keep_sending(&first, STALE + Duration::from_millis(300));
    let (second_kick, _) = kick();
    let refused = hub.publish_with("church", "main", "tab-2", None, "whip", Some(second_kick));
    let why = refused.err().expect("a live session is not taken over");
    assert!(why.contains("is already being published from tab-1"), "{why}");
    assert!(!kicked.load(Ordering::Relaxed));
    assert!(first.current());
}

#[test]
fn a_session_with_no_kick_is_never_taken_over() {
    let hub = Hub::new();
    let transcode = hub.publish_via("church", "small", "transcode", None, "transcode").unwrap();
    std::thread::sleep(STALE + Duration::from_millis(100));
    assert!(hub.held("church", "small"));
    let (k, _) = kick();
    assert!(hub.publish_with("church", "small", "obs", None, "rtmp", Some(k)).is_err());
    assert!(transcode.current());
}
