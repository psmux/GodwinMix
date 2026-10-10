use super::*;
use std::sync::Arc;

fn tag(kind: TagKind, ms: u32, keyframe: bool, body: &[u8]) -> MediaTag {
    MediaTag { kind, timestamp_ms: ms, keyframe, sequence_header: false, payload: Arc::from(body) }
}

#[test]
fn frames_keyframes_and_ac3_channels_are_read_off_the_tags() {
    let mut m = Meter::default();
    m.record(&tag(TagKind::Video, 0, true, &[0x17, 1, 0, 0, 0]));
    m.record(&tag(TagKind::Video, 40, false, &[0x27, 1, 0, 0, 0]));
    m.record(&tag(TagKind::Video, 1000, true, &[0x17, 1, 0, 0, 0]));
    let mut ac3 = crate::exaudio::prefix(crate::exaudio::AC3).to_vec();
    ac3.extend_from_slice(&[0x0B, 0x77, 0x00, 0x00, 0x1C, 0x40, 0xE1, 0x7F, 0x00]);
    m.record(&tag(TagKind::Audio, 0, false, &ac3));
    let mut s = InputStats::default();
    m.window -= std::time::Duration::from_secs(1);
    m.fill(&mut s);
    assert_eq!(s.keyframe_ms, Some(1000), "read off the stream's clock, not the wall's");
    assert_eq!(s.fps, 2.0, "three frames over a second of stream time");
    assert_eq!((s.audio_codec.as_str(), s.audio_channels), ("ac3", 6));
    assert!(s.last_frame_ms.unwrap() < 1000);
    assert!(m.has_video() && m.has_frames());
}

/// The second a sender restarts in, as a station test runner saw it: the
/// last frames before two quiet seconds and the first after them, which
/// read as 4.4 fps and a keyframe every 2.4 s. Neither is the feed's.
#[test]
fn a_window_with_a_stall_in_it_has_no_rate_and_no_keyframe_interval_across_it() {
    let mut m = Meter::default();
    let (key, inter) = ([0x17, 1, 0, 0, 0], [0x27, 1, 0, 0, 0]);
    for (n, ms) in [28_500, 28_533, 28_566].into_iter().enumerate() {
        m.record(&tag(TagKind::Video, ms, n == 0, if n == 0 { &key } else { &inter }));
    }
    for (n, ms) in [30_900, 30_933, 30_966].into_iter().enumerate() {
        m.record(&tag(TagKind::Video, ms, n == 0, if n == 0 { &key } else { &inter }));
    }
    let mut s = InputStats::default();
    m.window -= std::time::Duration::from_secs(1);
    m.fill(&mut s);
    assert_eq!(s.fps, 0.0, "no rate across the stall");
    assert_eq!(s.keyframe_ms, None, "no keyframe interval across it either");
    // The next second is the new sender alone, and reads true.
    for (n, ms) in (31_000..32_000).step_by(33).enumerate() {
        m.record(&tag(TagKind::Video, ms, n % 30 == 0, if n % 30 == 0 { &key } else { &inter }));
    }
    m.window -= std::time::Duration::from_secs(1);
    m.fill(&mut s);
    assert!((s.fps - 30.3).abs() < 0.5, "{}", s.fps);
}
