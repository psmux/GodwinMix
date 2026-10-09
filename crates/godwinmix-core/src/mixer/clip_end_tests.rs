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
    for name in ["repeat", "hold", "leave"] {
        assert_eq!(AtEnd::of(&params(&format!("at_end = \"{name}\""))).as_str(), name);
    }
}

fn segment_at(ms: u64) -> gst::Event {
    let _ = gst::init();
    let mut segment = gst::FormattedSegment::<gst::ClockTime>::new();
    segment.set_start(gst::ClockTime::from_mseconds(ms));
    segment.set_time(gst::ClockTime::from_mseconds(ms));
    gst::event::Segment::new(&segment)
}

fn eos() -> gst::Event {
    let _ = gst::init();
    gst::event::Eos::new()
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
        end.on_event(bit, &segment_at(0), &tell);
    }
    let video = end.on_event(VIDEO, &eos(), &tell);
    assert!(matches!(video, gst::PadProbeReturn::Drop), "an EOS must never reach a mixer pad");
    assert_eq!(told.load(Ordering::Relaxed), 0, "the sound is still playing");
    end.on_event(AUDIO, &eos(), &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1);
    assert!(end.at_end());
    end.on_event(AUDIO, &eos(), &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1, "one end, one notice");
    end.hold();
    assert!(end.held());

    // The seek back to the start flushes, then sends new segments, and the
    // next end is a new one. Playing again is neither holding nor at the end.
    end.fresh.store(true, Ordering::Release);
    for bit in [VIDEO, AUDIO] {
        end.on_event(bit, &segment_at(0), &tell);
    }
    assert!(!end.held() && !end.at_end(), "a clip playing again is not held");
    end.on_event(VIDEO, &eos(), &tell);
    end.on_event(AUDIO, &eos(), &tell);
    assert_eq!(told.load(Ordering::Relaxed), 2);
}

/// Seeked to its last frame, a clip's picture can be through its whole pass
/// before the sound's segment arrives. That segment belongs to the same pass
/// and is not a second end.
#[test]
fn a_late_segment_in_the_same_pass_is_not_a_new_end() {
    let end = ClipEnd::new();
    let told = AtomicU32::new(0);
    let tell = || {
        told.fetch_add(1, Ordering::Relaxed);
    };
    end.fresh.store(true, Ordering::Release);
    end.on_event(VIDEO, &segment_at(1_999), &tell);
    end.on_event(VIDEO, &eos(), &tell);
    end.on_event(AUDIO, &segment_at(1_999), &tell);
    end.on_event(AUDIO, &eos(), &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1, "one pass, one end");
    assert!(end.at_end());
}

/// Seeked to its last frame, a clip's branches can each be flushed more than
/// once, and the sound can reach its EOS between two flushes: one pass, two
/// notices. The mixer acts on the first and on no other until a new pass.
#[test]
fn a_pass_told_twice_is_acted_on_once() {
    let end = ClipEnd::new();
    let told = AtomicU32::new(0);
    let tell = || {
        told.fetch_add(1, Ordering::Relaxed);
    };
    for _ in 0..2 {
        end.fresh.store(true, Ordering::Release);
        end.on_event(AUDIO, &segment_at(1_966), &tell);
        end.on_event(AUDIO, &eos(), &tell);
    }
    assert_eq!(told.load(Ordering::Relaxed), 2, "the case this guards: two notices for one pass");
    assert!(end.act_once(), "the first notice is acted on");
    assert!(!end.act_once(), "the second is not");
    end.on_event(VIDEO, &segment_at(1_966), &tell);
    assert!(!end.act_once(), "the picture's late segment is the same pass");

    end.fresh.store(true, Ordering::Release);
    end.on_event(VIDEO, &segment_at(0), &tell);
    assert!(end.act_once(), "a new pass ends anew");
}

#[test]
fn a_clip_with_no_sound_is_not_waited_on_for_it() {
    let end = ClipEnd::new();
    let told = AtomicU32::new(0);
    let tell = || {
        told.fetch_add(1, Ordering::Relaxed);
    };
    end.on_event(VIDEO, &segment_at(0), &tell);
    end.on_event(VIDEO, &eos(), &tell);
    assert_eq!(told.load(Ordering::Relaxed), 1);
}

/// The EOS leaves the branch while the compositor still holds frames, and a
/// clip nothing paces would otherwise go round as fast as it decodes. The
/// last frame is due one duration after the pass's first segment, less where
/// that segment starts in the clip.
#[test]
fn the_last_frame_is_due_one_duration_after_the_pass_began() {
    let end = ClipEnd::new();
    let tell = || {};
    end.on_event(VIDEO, &segment_at(0), &tell);
    assert!(end.wait_for_last_frame(None).is_none(), "with no duration there is nothing to wait for");
    let wait = end.wait_for_last_frame(Some(8_000)).expect("eight seconds have not passed");
    assert!(wait > Duration::from_secs(7), "{wait:?}");
    // The other branch's segment is the same pass and does not move it.
    end.on_event(AUDIO, &segment_at(0), &tell);
    assert!(end.wait_for_last_frame(Some(8_000)).unwrap() > Duration::from_secs(7));

    // A scrub to seven seconds in: a flush, and a segment starting there.
    let scrubbed = ClipEnd::new();
    scrubbed.on_event(VIDEO, &segment_at(7_000), &tell);
    let wait = scrubbed.wait_for_last_frame(Some(8_000)).expect("one second is left");
    assert!(wait <= Duration::from_secs(1), "{wait:?}");
    let past = ClipEnd::new();
    past.on_event(VIDEO, &segment_at(9_000), &tell);
    assert!(past.wait_for_last_frame(Some(8_000)).is_none(), "an end that is due is acted on at once");
}
