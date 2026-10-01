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
