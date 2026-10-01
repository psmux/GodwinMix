use super::*;
use godwinmix_protocol::health::HealthState;

const BLACK: Look = Look { black_ratio: 1.0, mean: 16.0 };
const BUSY: Look = Look { black_ratio: 0.1, mean: 120.0 };

fn kinds(h: &Health) -> Vec<AlarmKind> {
    h.alarms.iter().map(|a| a.kind).collect()
}

fn live_judge() -> Judge {
    let mut j = Judge::new(Thresholds::default(), 0);
    j.live(true, 0);
    j
}

#[test]
fn a_show_with_no_input_says_so_and_nothing_else() {
    let j = Judge::new(Thresholds::default(), 500);
    let h = j.health(60_000);
    assert_eq!(kinds(&h), vec![AlarmKind::NoInput]);
    assert_eq!(h.alarms[0].since_ms, 500);
}

#[test]
fn black_is_raised_after_its_seconds_and_cleared_by_one_picture() {
    let mut j = live_judge();
    for t in 0..=4 {
        j.packet(t * 1000);
        j.picture(BLACK, Some(0.0), t * 1000);
    }
    let h = j.health(4_000);
    assert_eq!(kinds(&h), vec![AlarmKind::Black], "black hides freeze");
    assert_eq!(h.alarms[0].since_ms, 0);
    j.picture(BUSY, Some(0.2), 5_000);
    j.packet(5_000);
    assert_eq!(j.health(5_000).state, HealthState::Ok);
}

#[test]
fn a_still_picture_freezes_after_ten_seconds_and_motion_clears_it() {
    let mut j = live_judge();
    for t in 0..=10 {
        j.packet(t * 1000);
        j.picture(BUSY, Some(0.0005), t * 1000);
    }
    assert_eq!(kinds(&j.health(10_000)), vec![AlarmKind::Freeze]);
    j.picture(BUSY, Some(0.05), 11_000);
    j.packet(11_000);
    assert!(j.health(11_000).alarms.is_empty());
}

#[test]
fn quiet_sound_is_silence_after_its_seconds() {
    let mut j = live_judge();
    for t in 0..=10 {
        j.packet(t * 1000);
        j.sound(-90.0, t * 1000);
    }
    assert_eq!(kinds(&j.health(10_000)), vec![AlarmKind::Silence]);
    j.sound(-12.0, 10_500);
    assert!(j.health(10_500).alarms.is_empty());
}

#[test]
fn no_packet_for_the_stall_time_is_a_stall_and_hides_the_rest() {
    let mut j = live_judge();
    j.packet(1_000);
    j.picture(BLACK, Some(0.0), 1_000);
    let h = j.health(4_000);
    assert_eq!(kinds(&h), vec![AlarmKind::Stall]);
    assert_eq!(h.alarms[0].since_ms, 1_000);
}

#[test]
fn counters_raise_a_warning_inside_the_window_and_age_out() {
    let mut j = live_judge();
    j.counters(0, 0, 0);
    j.packet(1_000);
    j.counters(6, 3, 1_000);
    let h = j.health(1_000);
    assert_eq!(kinds(&h), vec![AlarmKind::CcErrors]);
    assert_eq!(h.state, HealthState::Warning);
    j.packet(20_000);
    j.counters(6, 3, 20_000);
    assert!(j.health(20_000).alarms.is_empty(), "the errors are out of the window");
}

#[test]
fn a_failed_output_stays_through_the_input_going_and_clears_when_it_recovers() {
    let mut j = live_judge();
    j.packet(0);
    j.output("yt", Some("connection refused"), 0);
    j.live(false, 1_000);
    let h = j.health(2_000);
    assert_eq!(kinds(&h), vec![AlarmKind::OutputFailed, AlarmKind::NoInput]);
    assert!(h.alarms[0].detail.contains("yt"));
    j.output("yt", None, 3_000);
    assert_eq!(kinds(&j.health(3_000)), vec![AlarmKind::NoInput]);
}

#[test]
fn a_check_with_zero_seconds_is_off() {
    let limits = Thresholds { black_secs: 0.0, ..Thresholds::default() };
    let mut j = Judge::new(limits, 0);
    j.live(true, 0);
    for t in 0..=30 {
        j.packet(t * 1000);
        j.picture(BLACK, Some(0.5), t * 1000);
    }
    assert!(j.health(30_000).alarms.is_empty());
}
