use super::*;
use godwinmix_sdk::wire::HealthState;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::atomic::AtomicU32;
use std::sync::mpsc;

fn test_pattern() -> Result<Capture, String> {
    let pipeline = crate::capture::build(
        "videotestsrc is-live=true ! video/x-raw,width=16,height=16,framerate=30/1 ! \
         queue name=gmx-video-queue ! fakesink sync=false",
    )?;
    Capture::start(pipeline, Some("gmx-video-queue"), None)
}

fn until(limit: Duration, mut f: impl FnMut() -> bool) -> bool {
    let end = Instant::now() + limit;
    while Instant::now() < end {
        if f() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    f()
}

#[test]
fn start_answers_before_a_slow_open_finishes_and_says_it_is_opening() {
    let asked = Instant::now();
    let opening = Opening::start(|_: &Cancel| {
        std::thread::sleep(Duration::from_millis(300));
        test_pattern()
    });
    assert!(
        asked.elapsed() < Duration::from_millis(100),
        "start waited for the open"
    );
    let health = opening.health("the camera");
    assert_eq!(health.state, HealthState::Degraded, "{health:?}");
    assert!(health.detail.unwrap_or_default().contains("is opening"));
    assert!(
        until(Duration::from_secs(3), || opening.is_open()),
        "it never opened"
    );
    assert_ne!(opening.health("the camera").state, HealthState::Failing);
    opening.stop(Duration::from_millis(10));
}

#[test]
fn a_failed_open_is_reported_by_health_with_the_reason_and_the_next_try() {
    let opening = Opening::start(|_: &Cancel| Err("another app is using it".into()));
    assert!(until(Duration::from_secs(2), || {
        opening.health("the camera").state == HealthState::Failing
    }));
    let detail = opening.health("the camera").detail.unwrap_or_default();
    assert!(detail.contains("another app is using it"), "{detail}");
    assert!(detail.contains("try"), "{detail}");
    opening.stop(Duration::from_millis(10));
}

#[test]
fn a_failed_open_is_tried_again_and_comes_up_by_itself() {
    let tries = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&tries);
    let opening = Opening::start(move |_: &Cancel| {
        if counted.fetch_add(1, Ordering::SeqCst) == 0 {
            return Err("the device is busy".into());
        }
        test_pattern()
    });
    let waited = RETRY_FIRST + Duration::from_secs(3);
    assert!(
        until(waited, || opening.is_open()),
        "the second try never opened it"
    );
    assert_eq!(tries.load(Ordering::SeqCst), 2);
    opening.stop(Duration::from_millis(10));
}

#[test]
fn stopping_during_an_open_does_not_wait_and_lets_go_of_what_it_opened() {
    let (made_tx, made) = mpsc::channel::<gst::Pipeline>();
    let (began_tx, began) = mpsc::channel::<()>();
    let opening = Opening::start(move |_: &Cancel| {
        let _ = began_tx.send(());
        std::thread::sleep(Duration::from_millis(300));
        let capture = test_pattern()?;
        let _ = made_tx.send(capture.pipeline().clone());
        Ok(capture)
    });
    began
        .recv_timeout(Duration::from_secs(2))
        .expect("the open began");
    let asked = Instant::now();
    opening.stop(Duration::from_millis(10));
    assert!(
        asked.elapsed() < Duration::from_millis(100),
        "stop waited for the open"
    );
    let pipeline = made
        .recv_timeout(Duration::from_secs(3))
        .expect("the open finished");
    // The open thread drops what it made the moment it sees the stop.
    assert!(
        until(Duration::from_secs(2), || pipeline.current_state()
            == gst::State::Null),
        "the device is still held: the pipeline is {:?}",
        pipeline.current_state()
    );
}

#[test]
fn a_new_open_that_replaces_one_not_yet_begun_skips_it() {
    // Replaced before its thread got going: the first open never asks for the
    // device at all, and the second is not held up by it.
    let (tx, rx) = mpsc::channel::<&'static str>();
    let first_tx = tx.clone();
    let first = Opening::start(move |_: &Cancel| {
        std::thread::sleep(Duration::from_millis(5));
        let _ = first_tx.send("first");
        Err("replaced".into())
    });
    let second = Opening::after(Some(first), move |_: &Cancel| {
        let _ = tx.send("second");
        test_pattern()
    });
    let mut heard = Vec::new();
    while let Ok(who) = rx.recv_timeout(Duration::from_secs(2)) {
        heard.push(who);
    }
    assert_eq!(heard.last(), Some(&"second"), "{heard:?}");
    assert!(
        heard.len() <= 2,
        "the replaced open ran more than once: {heard:?}"
    );
    second.stop(Duration::from_millis(10));
}

#[test]
fn stopping_while_it_waits_to_retry_stops_the_retries() {
    let tries = Arc::new(AtomicU32::new(0));
    let counted = Arc::clone(&tries);
    let opening = Opening::start(move |_: &Cancel| {
        counted.fetch_add(1, Ordering::SeqCst);
        Err("not there".into())
    });
    assert!(until(Duration::from_secs(1), || tries
        .load(Ordering::SeqCst)
        == 1));
    opening.stop(Duration::from_millis(10));
    std::thread::sleep(RETRY_FIRST + Duration::from_millis(300));
    assert_eq!(
        tries.load(Ordering::SeqCst),
        1,
        "it tried again after being stopped"
    );
}

#[test]
fn a_new_open_waits_for_the_one_it_replaces_to_let_go() {
    let (tx, rx) = mpsc::channel::<(&'static str, Instant)>();
    let first_tx = tx.clone();
    let (began_tx, began) = mpsc::channel::<()>();
    let first = Opening::start(move |_: &Cancel| {
        let _ = began_tx.send(());
        std::thread::sleep(Duration::from_millis(300));
        let _ = first_tx.send(("first finished", Instant::now()));
        Err("replaced".into())
    });
    began
        .recv_timeout(Duration::from_secs(2))
        .expect("the first open began");
    let second = Opening::after(Some(first), move |_: &Cancel| {
        let _ = tx.send(("second began", Instant::now()));
        test_pattern()
    });
    let a = rx.recv_timeout(Duration::from_secs(3)).expect("one event");
    let b = rx.recv_timeout(Duration::from_secs(3)).expect("two events");
    assert_eq!(a.0, "first finished");
    assert_eq!(b.0, "second began");
    assert!(b.1 >= a.1);
    assert!(until(Duration::from_secs(3), || second.is_open()));
    second.stop(Duration::from_millis(10));
}
