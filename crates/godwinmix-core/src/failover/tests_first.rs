//! A source's first connection: given the connect timeout before it counts
//! as down, and dated from when it was added once it does.

use super::*;

/// A board that gives a first connection 15 s, as the default config does.
fn board() -> Board {
    Board { connect_wait: Duration::from_secs(15), ..Default::default() }
}

#[test]
fn a_source_on_its_first_connection_is_not_down_until_the_connect_timeout() {
    let t = Instant::now();
    let mut b = board();
    b.adopt("new-cam", Some(&rule("wide", 3.0, 5.0)));
    let mut up = Up::with(&["wide"]);
    up.1 = Duration::from_secs(10);
    assert!(b.step(t, &up).is_empty(), "a camera slow to start was taken for one that went down");
    assert_eq!(b.state("new-cam", t, &up).unwrap().phase, BackupPhase::Ready);
    // Past the connect timeout it has not connected, and its outage is
    // counted from when it was added, so the backup goes on at once.
    up.1 = Duration::from_secs(15);
    assert_eq!(b.step(secs(t, 5.0), &up), vec![("new-cam".into(), Step::Engage)]);
    assert_eq!(apply(&mut b, &up)[0].to.as_deref(), Some("wide"));
    let s = b.state("new-cam", secs(t, 5.0), &up).unwrap();
    assert_eq!((s.phase, s.never_live, s.lost_ms), (BackupPhase::OnBackup, true, Some(15_000)));
    // It connects: an ordinary return, and the status no longer says it never did.
    up.0.insert("new-cam".into());
    b.step(secs(t, 6.0), &up);
    assert_eq!(b.step(secs(t, 11.0), &up), vec![("new-cam".into(), Step::Return)]);
    assert!(!b.state("new-cam", secs(t, 11.0), &up).unwrap().never_live);
}

#[test]
fn a_longer_after_secs_holds_a_first_connection_longer_than_the_connect_timeout() {
    let t = Instant::now();
    let mut b = board();
    b.adopt("new-cam", Some(&rule("wide", 20.0, 5.0)));
    let mut up = Up::with(&["wide"]);
    up.1 = Duration::from_secs(16);
    assert!(b.step(t, &up).is_empty(), "switched before after_secs");
    let s = b.state("new-cam", t, &up).unwrap();
    assert_eq!((s.phase, s.never_live, s.switch_in_ms), (BackupPhase::Lost, true, Some(4000)));
    assert_eq!(b.step(secs(t, 4.0), &up).len(), 1, "after_secs from when it was added had passed");
}

#[test]
fn a_first_connection_that_fails_counts_from_when_it_was_added() {
    let t = Instant::now();
    let mut b = board();
    b.adopt("cam", Some(&rule("wide", 3.0, 5.0)));
    // `cam` is down (failed) and has never been live, 4 s after it was added.
    let up = Up::with(&["wide"]);
    struct Failed<'a>(&'a Up);
    impl Liveness for Failed<'_> {
        fn live(&self, id: &str) -> bool { self.0.live(id) }
        fn down(&self, id: &str) -> bool { self.0.down(id) }
        fn live_within(&self, id: &str, w: Duration) -> bool { self.0.live_within(id, w) }
        fn unconnected_for(&self, id: &str) -> Option<Duration> { (id == "cam").then_some(Duration::from_secs(4)) }
    }
    assert_eq!(b.step(t, &Failed(&up)).len(), 1, "a camera that failed to start waited the connect timeout");
    assert!(b.never_live("cam"));
}
