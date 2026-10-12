use super::*;

#[test]
fn a_first_reading_of_the_rate_gives_way_to_the_measured_one_and_a_wobble_does_not() {
    let input = Some(json!({"state": "live", "since_ms": 1000, "video": {"codec": "h264", "fps": 8.0}}));
    let mut seen = Seen { input, ..Seen::default() };
    assert!(!seen.settle_fps(), "nothing measured yet");
    for fps in [29.97, 30.01, 29.98, 30.0] {
        seen.take_stats(&json!({"id": "a", "input": {"fps": fps}}));
        assert!(!seen.settle_fps(), "{fps} is not a steady rate yet");
    }
    seen.take_stats(&json!({"id": "a", "input": {"fps": 29.99}}));
    assert!(seen.settle_fps());
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 29.99);
    seen.take_stats(&json!({"id": "a", "input": {"fps": 30.02}}));
    assert!(!seen.settle_fps(), "a wobble is not a new rate");
    seen.input = Some(json!({"state": "live", "since_ms": u64::MAX, "video": {"fps": 0}}));
    assert!(!seen.settle_fps(), "an input live for less than a few seconds is left alone");
}

#[test]
fn the_second_a_sender_restarts_in_is_not_taken_for_its_rate() {
    let input = Some(json!({"state": "live", "since_ms": 1000, "video": {"codec": "h264", "fps": 30.0}}));
    let mut seen = Seen { input, ..Seen::default() };
    for fps in [30.0, 0.87, 30.0, 29.9, 10.1, 9.8, 0.42, 10.0, 10.2] {
        seen.take_stats(&json!({"id": "a", "input": {"fps": fps}}));
        assert!(!seen.settle_fps(), "{fps}");
    }
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 30.0);
}

/// A 30 fps feed whose stats have settled, as `direct.input` said it.
fn settled() -> Seen {
    let video = json!({"codec": "h264", "width": 320, "height": 180, "fps": 8.0, "kbps": 600});
    let mut seen = Seen::default();
    seen.take_input(&json!({"state": "live", "since_ms": 1000, "video": video}));
    for _ in 0..5 {
        seen.take_stats(&json!({"id": "a", "input": {"fps": 30.0}}));
        seen.settle_fps();
    }
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 30.0);
    seen
}

/// What the host says again when its input was read again from the start:
/// the hub's reading of the second around it.
fn again(fps: f64, width: u32, kbps: u32) -> Value {
    json!({"state": "live", "since_ms": 2000, "error": null,
           "video": {"codec": "h264", "width": width, "height": 180, "fps": fps, "kbps": kbps}})
}

#[test]
fn a_restarted_input_keeps_the_rate_its_stats_settled() {
    let mut seen = settled();
    assert!(!seen.take_input(&again(1.0, 320, 14)), "the second around a restart planned the rendition again");
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 30.0);
    let idle = json!({"state": "idle", "since_ms": 3000, "video": {"codec": "h264", "width": 320, "height": 180, "fps": 0.0}});
    assert!(seen.take_input(&idle), "an input gone idle is a move");
    assert!(seen.take_input(&again(3.0, 320, 90)));
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 30.0, "the rate was lost over an idle spell");
}

#[test]
fn a_new_picture_size_drops_the_settled_rate() {
    let mut seen = settled();
    assert!(seen.take_input(&again(25.0, 640, 900)));
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 25.0);
    assert!(seen.settled.is_none());
}

#[test]
fn a_rate_already_right_is_settled_too() {
    let video = json!({"codec": "h264", "width": 320, "height": 180, "fps": 29.0});
    let mut seen = Seen::default();
    seen.take_input(&json!({"state": "live", "since_ms": 1000, "video": video}));
    for _ in 0..5 {
        seen.take_stats(&json!({"id": "a", "input": {"fps": 30.0}}));
        assert!(!seen.settle_fps(), "29 is within a fifth of 30 and needs no new plan");
    }
    seen.take_input(&again(1.0, 320, 14));
    assert_eq!(seen.input.as_ref().unwrap()["video"]["fps"], 29.0);
}
