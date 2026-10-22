//! Sound and picture keep the distance the sender gave them, from the PES
//! times alone, and keep it across a sender that restarts on a new clock.

use super::feed;
use crate::direct::input::spec::Carry;
use crate::direct::input::tests::{clip, which};
use crate::media_tag::{MediaTag, TagKind};

/// A clip whose sound starts `offset` seconds after its picture, on a
/// clock that starts at `start` seconds.
fn offset_clip(name: &str, offset: &str, start: &str) -> Vec<u8> {
    let path = clip(name, &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-itsoffset", offset, "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
        "-t", "4", "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac", "-output_ts_offset", start, "-f", "mpegts"]);
    std::fs::read(path).unwrap()
}

/// What ffprobe says the distance is from the first picture's decode time
/// to the first sound's time, in ms, in `ts`.
fn truth(ts: &[u8]) -> i64 {
    let path = crate::direct::input::tests::scratch().join(format!("truth-{}.ts", ts.len()));
    std::fs::write(&path, ts).unwrap();
    let first = |sel: &str, key: &str| -> i64 {
        let out = std::process::Command::new("ffprobe").args(["-v", "error", "-select_streams", sel, "-show_entries", &format!("packet={key}"), "-of", "csv=p=0", "-read_intervals", "%+#1"]).arg(&path).output().unwrap();
        String::from_utf8_lossy(&out.stdout).trim().split(',').next().unwrap().parse().unwrap()
    };
    (first("a:0", "pts") - first("v:0", "dts")) / 90
}

/// `(first video frame, first sound frame)` times after `from`.
fn firsts(tags: &[MediaTag], from: u32) -> (u32, u32) {
    let first = |k: TagKind| tags.iter().find(|t| t.kind == k && !t.sequence_header && t.timestamp_ms >= from).map(|t| t.timestamp_ms).unwrap();
    (first(TagKind::Video), first(TagKind::Audio))
}

#[test]
fn a_known_offset_between_sound_and_picture_is_kept_to_the_millisecond() {
    if !which("ffmpeg") {
        return;
    }
    let ts = offset_clip("native-offset.ts", "0.320", "10");
    let tags = feed::tags(&ts, Carry::default());
    let (video, audio) = firsts(&tags, 0);
    let (apart, want) = (i64::from(audio) - i64::from(video), truth(&ts));
    assert!(want > 250, "the clip has its sound {want} ms late");
    assert!((apart - want).abs() <= 1, "sound came {apart} ms after picture, not {want}");
    // And at the end: the last frames are as far apart as the clip made them.
    let last = |k: TagKind| tags.iter().rev().find(|t| t.kind == k).unwrap().timestamp_ms;
    let (v, a) = (last(TagKind::Video), last(TagKind::Audio));
    assert!(a >= v && i64::from(a) - i64::from(v) <= 360, "no drift over the clip: {v} against {a}");
}

#[test]
fn a_sender_that_restarts_on_a_new_clock_carries_on_with_sound_and_picture_in_step() {
    if !which("ffmpeg") {
        return;
    }
    let one = offset_clip("native-clock-a.ts", "0.200", "1000");
    let two = offset_clip("native-clock-b.ts", "0.200", "3");
    let want = truth(&two);
    let alone = feed::tags(&one, Carry::default());
    let tags = feed::tags(&[one, two].concat(), Carry::default());
    let frames = |t: &[MediaTag], k: TagKind| -> Vec<u32> { t.iter().filter(|t| t.kind == k && !t.sequence_header).map(|t| t.timestamp_ms).collect() };
    let video = frames(&tags, TagKind::Video);
    let jump = video.windows(2).map(|w| i64::from(w[1]) - i64::from(w[0])).max().unwrap();
    let back = video.windows(2).map(|w| i64::from(w[1]) - i64::from(w[0])).min().unwrap();
    assert!(back >= 0 && jump < 200, "the picture's times went from step {back} to {jump} ms");
    // The second clip's first frames, on the carried on timeline, are as
    // far apart as they were in the clip. The first clip's last picture
    // ends only when the second clip starts, so the frame after the ones
    // read alone is the second clip's first.
    let n = frames(&alone, TagKind::Video).len();
    let v = video[n];
    let a = frames(&tags, TagKind::Audio)[frames(&alone, TagKind::Audio).len()];
    assert!((i64::from(a) - i64::from(v) - want).abs() <= 1, "after the restart: picture {v}, sound {a}, want {want} apart");
}
