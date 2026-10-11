//! The board's rules on a clock that is moved by hand.

use super::*;
use godwinmix_protocol::backup::{BackupPhase, ReturnMode};
use std::collections::HashSet;

/// The live sources, and how long every source named `new-` that is not
/// live has been making its first connection.
#[derive(Default)]
struct Up(HashSet<String>, Duration);

impl Up {
    fn with(ids: &[&str]) -> Self {
        Up(ids.iter().map(|s| s.to_string()).collect(), Duration::ZERO)
    }
}

impl Liveness for Up {
    fn live(&self, id: &str) -> bool {
        self.0.contains(id)
    }
    /// Anything not live is down here, except a name starting `new-`, which
    /// is on its first connection.
    fn down(&self, id: &str) -> bool {
        !self.0.contains(id) && !id.starts_with("new-")
    }
    fn live_within(&self, id: &str, _: Duration) -> bool {
        self.0.contains(id)
    }
    fn unconnected_for(&self, id: &str) -> Option<Duration> {
        (id.starts_with("new-") && !self.0.contains(id)).then_some(self.1)
    }
}

#[path = "tests_first.rs"]
mod first;

fn rule(backup: &str, after: f64, back: f64) -> OnLoss {
    OnLoss { after_secs: after, return_after_secs: back, ..OnLoss::to(backup) }
}

fn secs(at: Instant, s: f64) -> Instant {
    at + Duration::from_secs_f64(s)
}

/// Apply every change the board asks for, as the mixer would.
fn apply(board: &mut Board, live: &Up) -> Vec<Swap> {
    let changes = board.changes(live);
    for c in &changes {
        board.shown(&c.source, c.to.clone());
    }
    changes
}

#[test]
fn the_backup_goes_on_after_the_hold_and_comes_off_once_steady() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 3.0, 5.0)));
    let mut up = Up::with(&["main", "wide"]);
    assert!(b.step(t, &up).is_empty());
    up.0.remove("main");
    assert!(b.step(secs(t, 1.0), &up).is_empty());
    assert_eq!(b.state("main", secs(t, 2.0), &up).unwrap().phase, BackupPhase::Lost);
    assert!(b.step(secs(t, 3.5), &up).is_empty(), "held only 2.5 s");
    assert_eq!(b.step(secs(t, 4.1), &up), vec![("main".into(), Step::Engage)]);
    assert_eq!(apply(&mut b, &up)[0].to.as_deref(), Some("wide"));
    assert_eq!(b.stand_in("main"), Some("wide"));
    up.0.insert("main".into());
    b.step(secs(t, 10.0), &up);
    assert_eq!(b.state("main", secs(t, 11.0), &up).unwrap().return_in_ms, Some(4000));
    assert!(b.step(secs(t, 14.0), &up).is_empty(), "back for only 4 s");
    assert_eq!(b.step(secs(t, 15.0), &up), vec![("main".into(), Step::Return)]);
    let off = b.changes(&up);
    assert_eq!(off[0].to, None);
    assert_eq!(b.shown("main", None), Some("returned"));
}

#[test]
fn a_feed_that_flickers_does_not_return_until_it_holds() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 0.0, 5.0)));
    let mut up = Up::with(&["wide"]);
    assert_eq!(b.step(t, &up).len(), 1, "after_secs 0 switches at once");
    for i in 0..10 {
        let on = i % 2 == 0;
        if on { up.0.insert("main".into()); } else { up.0.remove("main"); }
        assert!(b.step(secs(t, 1.0 + i as f64 * 2.0), &up).is_empty(), "returned on a flicker at {i}");
    }
}

#[test]
fn manual_return_holds_until_the_operator_and_stay_turns_auto_into_manual() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&OnLoss { returns: ReturnMode::Manual, ..rule("wide", 0.0, 0.0) }));
    let mut up = Up::with(&["wide"]);
    b.step(t, &up);
    up.0.insert("main".into());
    assert!(b.step(secs(t, 60.0), &up).is_empty());
    assert_eq!(b.state("main", secs(t, 60.0), &up).unwrap().phase, BackupPhase::MainBack);
    b.release("main").unwrap();
    assert!(!b.engaged("main"));
    assert!(b.release("main").is_err(), "a second return has nothing to return from");

    b.set("main", Some(rule("wide", 0.0, 1.0)));
    up.0.remove("main");
    b.step(secs(t, 70.0), &up);
    b.stay("main").unwrap();
    up.0.insert("main".into());
    assert!(b.step(secs(t, 80.0), &up).is_empty());
    assert!(b.step(secs(t, 90.0), &up).is_empty(), "stay was ignored");
    assert_eq!(b.state("main", secs(t, 90.0), &up).unwrap().phase, BackupPhase::MainBack);
}

#[test]
fn a_backup_that_is_down_too_hands_on_to_its_own_backup() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 0.0, 5.0)));
    b.adopt("wide", Some(&rule("slate-clip", 3.0, 5.0)));
    let mut up = Up::with(&["slate-clip"]);
    b.step(t, &up);
    assert_eq!(b.chain("main"), vec!["wide".to_string(), "slate-clip".to_string()]);
    assert_eq!(apply(&mut b, &up)[0].to.as_deref(), Some("slate-clip"));
    // The first backup coming back does not swap a picture that is fine.
    up.0.insert("wide".into());
    assert!(b.changes(&up).iter().all(|c| c.source != "main"));
    // Nothing live at all: the backup comes off and the status says why.
    let none = Up::default();
    assert_eq!(apply(&mut b, &none)[0].to, None);
    let s = b.state("main", t, &none).unwrap();
    assert!(s.warning.unwrap().contains("has no picture either"));
}

#[test]
fn a_rebuild_keeps_what_the_operator_set_and_a_removal_forgets_it() {
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 3.0, 5.0)));
    b.set("main", None);
    b.adopt("main", Some(&rule("wide", 3.0, 5.0)));
    assert!(b.rule("main").is_none(), "the rebuild brought back a cleared rule");
    assert!(!b.busy(), "a cleared rule still costs a tick");
    b.forget("main");
    b.adopt("main", Some(&rule("other", 3.0, 5.0)));
    assert_eq!(b.rule("main").unwrap().backup, "other");
}

#[test]
fn clearing_a_rule_on_air_takes_the_backup_off() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 0.0, 5.0)));
    let up = Up::with(&["wide"]);
    b.step(t, &up);
    apply(&mut b, &up);
    b.set("main", None);
    assert_eq!(b.changes(&up)[0].to, None);
    assert_eq!(b.shown("main", None), Some("cleared"));
    assert!(!b.busy());
}

#[test]
fn the_status_warns_before_it_matters_when_the_backup_is_down() {
    let t = Instant::now();
    let mut b = Board::default();
    b.adopt("main", Some(&rule("wide", 3.0, 5.0)));
    let up = Up::with(&["main"]);
    b.step(t, &up);
    let s = b.state("main", t, &up).unwrap();
    assert_eq!(s.phase, BackupPhase::Ready);
    assert!(s.warning.unwrap().contains("could not stand in"));
}
