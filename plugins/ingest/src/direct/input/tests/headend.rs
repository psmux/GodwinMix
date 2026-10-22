//! A broadcast multiplex as a headend sees one: MPEG-2 video and three
//! AC-3 tracks with their languages, one of them 5.1.

use serde_json::json;

use super::*;
use crate::direct::input::Carry;

/// A 704x480 29.97 MPEG-2 clip with English, Spanish and French AC-3, the
/// French in 5.1, in MPEG-TS.
pub fn mpeg2_clip() -> PathBuf {
    clip("mpeg2-ac3x3.ts", &["-f", "lavfi", "-i", "testsrc2=size=704x480:rate=30000/1001", "-f", "lavfi", "-i", "sine=frequency=440",
        "-f", "lavfi", "-i", "sine=frequency=660", "-f", "lavfi", "-i", "sine=frequency=880", "-t", "3",
        "-map", "0", "-map", "1", "-map", "2", "-map", "3", "-c:v", "mpeg2video", "-b:v", "3M", "-g", "15", "-flags", "+ilme+ildct",
        "-c:a", "ac3", "-b:a", "192k", "-ac:a:2", "6", "-metadata:s:a:0", "language=eng", "-metadata:s:a:1", "language=spa",
        "-metadata:s:a:2", "language=fra", "-f", "mpegts"])
}

fn start_carrying(uri: &str, carry: Carry) -> Running {
    let mut spec = InputSpec::from_json(&json!(uri)).unwrap();
    spec.carry = carry;
    let input = open(&spec, &Context::default()).unwrap_or_else(|e| panic!("{e}"));
    let (got, stop) = (Collect::default(), StopSignal::default());
    let (sink, s) = (Box::new(got.clone()), stop.clone());
    let thread = std::thread::spawn(move || input.run(sink, s));
    Running { got, stop, thread: Some(thread) }
}

#[test]
fn a_headend_input_carries_mpeg2_and_every_sound_track_numbered_with_its_language() {
    if !which("ffmpeg") {
        return;
    }
    let url = file_url(&mpeg2_clip());
    let rx = start_carrying(&url, Carry { mpeg2: true, tracks: true });
    assert!(eventually(10, || rx.got.keyframes() >= 3), "no MPEG-2 keyframes");
    std::thread::sleep(Duration::from_secs(1));
    let got = rx.got.0.lock().unwrap();
    let video: Vec<&MediaTag> = got.tags.iter().filter(|t| t.kind == TagKind::Video).collect();
    assert!(video.iter().all(|t| crate::mp2v::is(&t.payload) && t.track == 0));
    let s = crate::mp2v::sequence(&video.iter().find(|t| t.keyframe).unwrap().payload[crate::mp2v::PREFIX..]).unwrap();
    assert_eq!((s.width, s.height, s.fps, s.progressive), (704, 480, (30000, 1001), false));
    for track in 0..3u8 {
        let n = got.tags.iter().filter(|t| t.kind == TagKind::Audio && t.track == track).count();
        assert!(n > 20, "track {track} carried {n} frames");
    }
    let stats = got.stats.last().cloned().unwrap();
    let langs: Vec<(u8, Option<String>)> = stats.tracks.iter().filter(|t| t.kind == "audio").map(|t| (t.track, t.lang.clone())).collect();
    assert_eq!(langs, [(0, Some("eng".into())), (1, Some("spa".into())), (2, Some("fra".into()))], "{:?}", stats.tracks);
    assert!(stats.tracks.iter().all(|t| t.pid.is_some()), "{:?}", stats.tracks);
}

#[test]
fn without_carry_the_input_is_what_it_was_one_sound_and_no_mpeg2() {
    if !which("ffmpeg") {
        return;
    }
    let rx = start_carrying(&file_url(&mpeg2_clip()), Carry::default());
    assert!(eventually(10, || rx.got.frames(TagKind::Audio) >= 30), "no sound");
    let got = rx.got.0.lock().unwrap();
    assert!(got.tags.iter().all(|t| t.track == 0 && t.kind != TagKind::Video));
    let error = got.stats.last().and_then(|s| s.error.clone()).unwrap_or_default();
    assert!(error.contains("video/mpeg"), "the error names what is left out: {error}");
}
