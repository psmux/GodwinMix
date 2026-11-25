//! Hellos while nothing answers: the growing wait, and the reasons a caller
//! is shown.

use std::time::{Duration, Instant};

use super::tests::{drain, opened, switcher_hello, SESSION};
use super::*;

#[test]
fn no_answer_says_so_and_waits_longer_each_time() {
    let start = Instant::now();
    let mut s = Session::new(SESSION, start);
    let mut sent_at = Vec::new();
    let mut now = start;
    while now < start + Duration::from_secs(30) {
        s.handle_timeout(now);
        if !drain(&mut s).is_empty() {
            sent_at.push((now - start).as_secs());
        }
        now = s.poll_timeout();
    }
    // One a second for the first five seconds, then 2, 4 and 8 apart.
    assert_eq!(sent_at, [0, 1, 2, 3, 4, 5, 6, 8, 12, 20, 28]);
    assert_eq!(s.link().reason.as_deref(), Some(reasons::NO_ANSWER));
}

#[test]
fn an_answer_puts_the_wait_back_to_a_second() {
    let start = Instant::now();
    let mut s = opened(start);
    let later = start + Duration::from_secs(5);
    s.handle_timeout(later);
    assert_eq!(s.link().reason.as_deref(), Some(reasons::SILENT));
    s.handle_timeout(later);
    drain(&mut s);
    assert_eq!(s.poll_timeout(), later + Duration::from_secs(1));
}

#[test]
fn nothing_listening_is_said_while_saying_hello_and_kept() {
    let start = Instant::now();
    let mut s = Session::new(SESSION, start);
    s.handle_timeout(start);
    s.handle_unreachable();
    assert_eq!(s.link().reason.as_deref(), Some(reasons::REFUSED));
    for second in 1..=10 {
        s.handle_timeout(start + Duration::from_secs(second));
    }
    assert_eq!(s.link().reason.as_deref(), Some(reasons::REFUSED));
    s.handle_datagram(&switcher_hello(2), start + Duration::from_secs(10));
    assert_eq!(s.link().reason, None);
    s.handle_unreachable();
    assert_eq!(s.link().reason, None, "an open session waits for silence");
}

#[test]
fn a_faster_timing_divides_every_wait() {
    let t = Timing::default().faster(10);
    assert_eq!(t.hello_every, Duration::from_millis(100));
    assert_eq!(t.silent_after, Duration::from_millis(500));
    assert_eq!(t.max_resends, 10);
}
