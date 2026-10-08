use super::*;
use std::sync::atomic::AtomicU32;

fn params(s: &str) -> Params {
    toml::from_str(s).unwrap()
}

#[test]
fn a_clip_holds_at_its_end_unless_its_params_say_otherwise() {
    assert_eq!(AtEnd::of(&params("")), AtEnd::Hold, "a new clip holds its last frame");
    assert_eq!(AtEnd::of(&params("at_end = \"repeat\"")), AtEnd::Repeat);
    assert_eq!(AtEnd::of(&params("at_end = \"leave\"")), AtEnd::Leave);
    assert_eq!(AtEnd::of(&params("at_end = \"hold\"")), AtEnd::Hold);
    // What the OBS import wrote for a looping media source before at_end.
    assert_eq!(AtEnd::of(&params("loop = true")), AtEnd::Repeat);
    assert_eq!(AtEnd::of(&params("loop = true\nat_end = \"hold\"")), AtEnd::Hold, "at_end wins");
    for name in AtEnd::NAMES {
        assert_eq!(AtEnd::of(&params(&format!("at_end = \"{name}\""))).as_str(), name);
    }
}

/// The end is said once, when every branch that played has ended, and the EOS
/// never reaches the compositor or the audio mixer.
#[test]
fn the_end_is_told_once_when_the_last_branch_ends() {
    let end = ClipEnd::new();
    let told = AtomicU32::new(0);
    let tell = || {
        told.fetch_add(1, Ordering::Relaxed);
    };
    for bit in [VIDEO, AUDIO] {
        end.on_event(bit, gst::EventType::Segment, &tell);
    }
    let video = end.on_event(VIDEO, gst::EventType::Eos, &tell);
    assert!(matches!(video, gst::PadProbeReturn::Drop), "an EOS must never reach a mixer pad");
    assert_eq!(told.load(Ordering::Relaxed), 0, "the sound is still playing");
    end.on_event(AUDIO, gst::EventType::Eos, &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1);
    end.on_event(AUDIO, gst::EventType::Eos, &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1, "one end, one notice");
    end.hold();
    assert!(end.held());

    // The seek back to the start sends new segments, and the next end is a
    // new one. Playing again is not holding.
    for bit in [VIDEO, AUDIO] {
        end.on_event(bit, gst::EventType::Segment, &tell);
    }
    assert!(!end.held(), "a clip playing again is not held");
    end.on_event(VIDEO, gst::EventType::Eos, &tell);
    end.on_event(AUDIO, gst::EventType::Eos, &tell);
    assert_eq!(told.load(Ordering::Relaxed), 2);
}

#[test]
fn a_clip_with_no_sound_is_not_waited_on_for_it() {
    let end = ClipEnd::new();
    let told = AtomicU32::new(0);
    let tell = || {
        told.fetch_add(1, Ordering::Relaxed);
    };
    end.on_event(VIDEO, gst::EventType::Segment, &tell);
    end.on_event(VIDEO, gst::EventType::Eos, &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1);
}

/// A clip nothing paces would otherwise go round as fast as it decodes.
#[test]
fn a_repeat_waits_out_the_rest_of_the_clip_when_the_end_came_early() {
    let end = ClipEnd::new();
    assert!(end.wait_before_repeat(None).is_none(), "with no duration there is nothing to wait for");
    let wait = end.wait_before_repeat(Some(8_000)).expect("eight seconds have not passed");
    assert!(wait > Duration::from_secs(7), "{wait:?}");
    // A seek to seven seconds in puts the start seven seconds back.
    end.landed_at(7_000);
    let wait = end.wait_before_repeat(Some(8_000)).expect("one second is left");
    assert!(wait <= Duration::from_secs(1), "{wait:?}");
    end.landed_at(9_000);
    assert!(end.wait_before_repeat(Some(8_000)).is_none(), "an end that is due repeats at once");
}
