use super::*;

const TICK: Duration = Duration::from_millis(500);
const BASE: Duration = Duration::from_secs(10);

fn ticks_to_restart(p: &mut Patience, loaded: bool) -> u32 {
    (1..=10_000).find(|_| p.stalled_tick(TICK, loaded, BASE)).expect("restarted eventually")
}

#[test]
fn each_strike_doubles_the_wait_up_to_the_ceiling() {
    let mut p = Patience::default();
    let mut waits = Vec::new();
    for _ in 0..7 {
        waits.push(ticks_to_restart(&mut p, false) as u64 * TICK.as_millis() as u64 / 1000);
        p.struck();
    }
    assert_eq!(waits, [10, 20, 40, 80, 160, 160, 160]);
}

#[test]
fn a_starved_programme_slows_the_count_without_stopping_it() {
    let mut p = Patience::default();
    assert_eq!(ticks_to_restart(&mut p, true), 80, "four times the ticks, and still restarted");
}

#[test]
fn a_frame_or_two_between_stalls_forgives_nothing() {
    let mut p = Patience::default();
    let start = Instant::now();
    for n in 0..5u64 {
        ticks_to_restart(&mut p, false);
        p.struck();
        // Back for two seconds, the shape of the page on 2026-10-05.
        assert!(!p.healthy_tick(true, start + Duration::from_secs(n * 100)));
        assert!(!p.healthy_tick(true, start + Duration::from_secs(n * 100 + 2)));
    }
    assert_eq!(p.strikes(), 5);
    assert_eq!(p.stall_limit(BASE), Duration::from_secs(160));
}

#[test]
fn a_minute_live_clears_the_strikes() {
    let mut p = Patience::default();
    p.struck();
    p.struck();
    let t = Instant::now();
    assert!(!p.healthy_tick(true, t));
    assert!(!p.healthy_tick(true, t + HEALTHY_FOR / 2));
    assert!(p.healthy_tick(true, t + HEALTHY_FOR));
    assert!(p.forgiven());
    assert_eq!(p.stall_limit(BASE), BASE);
}

#[test]
fn a_stall_in_the_middle_starts_the_healthy_minute_again() {
    let mut p = Patience::default();
    p.struck();
    let t = Instant::now();
    p.healthy_tick(true, t);
    p.stalled_tick(TICK, false, BASE);
    assert!(!p.healthy_tick(true, t + HEALTHY_FOR));
    assert!(p.healthy_tick(true, t + HEALTHY_FOR * 2));
}

#[test]
fn starvation_is_under_half_the_frames() {
    let half_second = Duration::from_millis(500);
    assert!(!programme_starved(15, half_second, 30.0));
    assert!(!programme_starved(8, half_second, 30.0));
    assert!(programme_starved(7, half_second, 30.0));
    assert!(programme_starved(0, half_second, 30.0));
    assert!(!programme_starved(0, Duration::from_millis(50), 30.0), "too short to say");
}
