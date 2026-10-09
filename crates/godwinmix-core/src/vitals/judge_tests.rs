use super::*;
use godwinmix_protocol::health::HealthState;

fn kinds(h: &Health) -> Vec<AlarmKind> {
    h.alarms.iter().map(|a| a.kind).collect()
}

fn output(id: &str, state: OutputState, shed: Option<&str>) -> OutputStatus {
    serde_json::from_value(serde_json::json!({
        "id": id, "uri_host": "example.com", "has_key": true, "state": state,
        "reconnects": 0, "queue_secs": 0.0, "shed": shed,
    }))
    .unwrap()
}

#[test]
fn a_black_programme_is_an_alarm_after_its_seconds() {
    let mut j = Judge::new(Thresholds::default());
    for t in 0..=4 {
        j.picture(1.0, Some(0.0), t * 1000);
    }
    let h = j.health(4_000);
    assert_eq!(kinds(&h), vec![AlarmKind::Black], "black hides freeze");
    j.picture(0.1, Some(0.1), 5_000);
    assert_eq!(j.health(5_000).state, HealthState::Ok);
}

#[test]
fn a_still_programme_freezes_and_no_picture_clears_it() {
    let mut j = Judge::new(Thresholds::default());
    for t in 0..=10 {
        j.picture(0.2, Some(0.0001), t * 1000);
    }
    assert_eq!(kinds(&j.health(10_000)), vec![AlarmKind::Freeze]);
    j.no_picture();
    assert!(j.health(11_000).alarms.is_empty());
}

#[test]
fn quiet_programme_sound_is_silence() {
    let mut j = Judge::new(Thresholds::default());
    for t in 0..=100 {
        j.sound(-80.0, t * 100);
    }
    assert_eq!(kinds(&j.health(10_000)), vec![AlarmKind::Silence]);
    j.sound(-20.0, 10_100);
    assert!(j.health(10_100).alarms.is_empty());
}

#[test]
fn no_sound_on_programme_is_not_silence_and_judging_starts_again_from_then() {
    let mut j = Judge::new(Thresholds::default());
    for t in 0..=100 {
        j.sound(-80.0, t * 100);
    }
    j.no_sound();
    assert!(j.health(10_000).alarms.is_empty(), "the slate has nothing to fall quiet");
    for t in 101..=150 {
        j.sound(-80.0, t * 100);
    }
    assert!(j.health(15_000).alarms.is_empty(), "five seconds quiet since a source came on is not ten");
}

#[test]
fn failed_and_shed_outputs_are_named_and_keep_their_start() {
    let mut j = Judge::new(Thresholds::default());
    j.outputs(&[output("yt", OutputState::Failed, None), output("fb", OutputState::Live, Some("the CPU is full"))], 1_000);
    j.outputs(&[output("yt", OutputState::Failed, None), output("fb", OutputState::Live, Some("the CPU is full"))], 2_000);
    let h = j.health(2_000);
    assert_eq!(kinds(&h), vec![AlarmKind::OutputFailed, AlarmKind::Shed]);
    assert!(h.alarms.iter().all(|a| a.since_ms == 1_000));
    assert!(h.alarms[0].detail.contains("yt"));
    j.outputs(&[output("yt", OutputState::Live, None)], 3_000);
    assert!(j.health(3_000).alarms.is_empty());
}

#[test]
fn the_memory_guard_is_an_alarm_until_it_clears() {
    let mut j = Judge::new(Thresholds::default());
    j.memory(Some((4_000, "The show holds 4200 MB, past its 4096 MB guard.".into())));
    let h = j.health(5_000);
    assert_eq!(kinds(&h), vec![AlarmKind::Memory]);
    assert_eq!(h.state, HealthState::Alarm);
    assert_eq!(h.alarms[0].since_ms, 4_000);
    j.memory(None);
    assert!(j.health(6_000).alarms.is_empty());
}
