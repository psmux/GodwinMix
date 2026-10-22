use super::*;

fn pad(name: &str) -> gst::Pad {
    gst::Pad::builder(gst::PadDirection::Src).name(name).build()
}

#[test]
fn a_stream_waiting_for_its_slot_takes_it_when_the_holder_goes() {
    gst::init().unwrap();
    let s = Streams::default();
    let (v1, a1, v2, a2, a3) = (pad("v1"), pad("a1"), pad("v2"), pad("a2"), pad("a3"));
    let on_v1 = s.add(&v1, Slot::Video, "video/x-h264", Vec::new());
    let on_a1 = s.add(&a1, Slot::Audio, "audio/mpeg", Vec::new());
    let on_v2 = s.add(&v2, Slot::Video, "video/x-h264", Vec::new());
    let on_a2 = s.add(&a2, Slot::Audio, "audio/mpeg", Vec::new());
    let on_a3 = s.add(&a3, Slot::Audio, "audio/mpeg", Vec::new());
    assert!(on_v1.on.load(Ordering::Relaxed) && on_a1.on.load(Ordering::Relaxed));
    assert!(!on_v2.on.load(Ordering::Relaxed) && !on_a2.on.load(Ordering::Relaxed));
    assert_eq!(s.notes().len(), 3, "{:?}", s.notes());
    s.removed(&a1);
    s.removed(&v1);
    assert!(on_v2.on.load(Ordering::Relaxed) && on_a2.on.load(Ordering::Relaxed) && !on_a3.on.load(Ordering::Relaxed));
    assert_eq!(s.notes(), ["a second audio stream (audio/mpeg); the first is taken"]);
}

#[test]
fn a_waiting_stream_takes_the_slot_of_a_holder_that_went_quiet_and_only_then() {
    gst::init().unwrap();
    let s = Streams::default();
    let (v1, v2) = (pad("v1"), pad("v2"));
    let first = s.add(&v1, Slot::Video, "video/x-h264", Vec::new());
    let second = s.add(&v2, Slot::Video, "video/x-h264", Vec::new());
    let now = 10 * QUIET_MS;
    first.last_ms.store(now - 100, Ordering::Relaxed);
    assert!(!s.claim_at(&second, now), "a holder still giving keeps its slot");
    first.last_ms.store(now - QUIET_MS - 100, Ordering::Relaxed);
    assert!(s.claim_at(&second, now), "a holder quiet for longer than QUIET_MS gives it up");
    assert!(second.on.load(Ordering::Relaxed) && !first.on.load(Ordering::Relaxed));
    assert!(s.notes().is_empty(), "{:?}", s.notes());
}

#[test]
fn a_stream_left_out_is_forgotten_when_it_goes() {
    gst::init().unwrap();
    let s = Streams::default();
    let (v1, t) = (pad("v1"), pad("t"));
    s.add(&v1, Slot::Video, "video/x-h264", Vec::new());
    assert!(!s.add(&t, Slot::None, "private/teletext", Vec::new()).on.load(Ordering::Relaxed));
    assert_eq!(s.notes(), ["private/teletext, which a direct show does not carry"]);
    s.removed(&t);
    assert!(s.notes().is_empty());
}
