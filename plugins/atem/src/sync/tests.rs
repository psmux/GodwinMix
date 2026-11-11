//! Follow, lead and the guard, with no core and no switcher.

use std::time::{Duration, Instant};

use super::*;

fn seen(id: &str, mode: Mode, program: u16, map: &[(&str, u16)]) -> Seen {
    let map = map.iter().map(|(s, i)| (s.to_string(), *i)).collect();
    Seen { id: id.into(), mode, up: true, sessions: 1, program: Some(program), map }
}

#[test]
fn a_take_cuts_only_following_switchers_that_map_it_and_need_it() {
    let all = [
        seen("atem-main", Mode::Follow, 1, &[("cam-a", 1), ("cam-b", 2)]),
        seen("atem-overflow", Mode::Follow, 2, &[("cam-b", 2)]),
        seen("atem-own", Mode::Own, 1, &[("cam-b", 3)]),
        seen("atem-lead", Mode::Lead, 1, &[("cam-b", 3)]),
    ];
    assert_eq!(follow(&all, "cam-b"), [("atem-main".to_string(), 2)], "overflow is there already");
    assert!(follow(&all, "slides").is_empty(), "nothing maps slides");
    let mut down = all.clone();
    down[0].up = false;
    assert!(follow(&down, "cam-b").is_empty(), "a switcher that is down is not sent to");
}

#[test]
fn a_leading_switcher_moves_the_mix_only_when_its_programme_changes() {
    let mut lead = Lead::default();
    let mut s = vec![seen("atem-main", Mode::Lead, 1, &[("cam-a", 1), ("cam-b", 2)])];
    assert!(lead.changes(&s, None).is_empty(), "the first look only notes where it is");
    s[0].program = Some(2);
    assert_eq!(lead.changes(&s, Some("cam-a")), [("atem-main".to_string(), "cam-b".to_string())]);
    assert!(lead.changes(&s, Some("cam-b")).is_empty(), "no change, no take");
    s[0].program = Some(5);
    assert!(lead.changes(&s, Some("cam-b")).is_empty(), "an unmapped input moves nothing");
    s[0].program = Some(1);
    assert!(lead.changes(&s, Some("cam-a")).is_empty(), "GodwinMix has it already");
}

#[test]
fn a_new_session_is_noted_not_taken() {
    let mut lead = Lead::default();
    let mut s = vec![seen("atem-main", Mode::Lead, 1, &[("cam-a", 1), ("cam-b", 2)])];
    lead.changes(&s, None);
    (s[0].program, s[0].sessions) = (Some(2), 2);
    assert!(lead.changes(&s, None).is_empty(), "the dump after a reconnect is not a cut");
}

#[test]
fn a_follow_and_a_lead_switcher_do_not_chase_each_other() {
    // atem-lead's panel cuts to input 2, GodwinMix takes cam-b, atem-follow
    // cuts to its input for cam-b. The take event for cam-b moves nothing
    // further: the follower is there, and the leader does not follow.
    let mut lead = Lead::default();
    let mut all = vec![
        seen("atem-lead", Mode::Lead, 1, &[("cam-a", 1), ("cam-b", 2)]),
        seen("atem-follow", Mode::Follow, 4, &[("cam-a", 4), ("cam-b", 5)]),
    ];
    lead.changes(&all, Some("cam-a"));
    all[0].program = Some(2);
    let takes = lead.changes(&all, Some("cam-a"));
    assert_eq!(takes, [("atem-lead".to_string(), "cam-b".to_string())]);
    assert_eq!(follow(&all, "cam-b"), [("atem-follow".to_string(), 5)]);
    all[1].program = Some(5);
    assert!(follow(&all, "cam-b").is_empty());
    assert!(lead.changes(&all, Some("cam-b")).is_empty());
}

#[test]
fn the_guard_trips_past_six_moves_a_second_and_comes_back() {
    let start = Instant::now();
    let mut g = Guard::default();
    for i in 0..6 {
        assert_eq!(g.check(start + Duration::from_millis(i * 10)), Verdict::Go);
    }
    assert_eq!(g.check(start + Duration::from_millis(70)), Verdict::Tripped);
    assert_eq!(g.check(start + Duration::from_millis(500)), Verdict::Paused);
    assert!(g.paused(start + Duration::from_secs(2)));
    assert_eq!(g.check(start + Duration::from_secs(4)), Verdict::Go);
    // Moves spread out never trip it.
    let mut g = Guard::default();
    for i in 0..30 {
        assert_eq!(g.check(start + Duration::from_millis(i * 200)), Verdict::Go);
    }
}
