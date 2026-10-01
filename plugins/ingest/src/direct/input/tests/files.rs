//! A looped file, a channel stream off the hub, and a backup taking over.

use std::sync::Arc;

use serde_json::json;

use super::*;
use crate::hub::Hub;

/// A two second clip played for five: paced to the clock, looped, and
/// time running on across the loop.
#[test]
fn a_file_loops_at_its_own_pace_with_time_running_on() {
    if !which("ffmpeg") {
        return;
    }
    for name in ["loop.ts", "loop.mp4"] {
        let clip = clip(name, &["-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine", "-t", "2",
            "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac"]);
        let rx = start(json!(format!("file://{}", path(&clip))), &Context::default());
        std::thread::sleep(Duration::from_secs(5));
        let times = rx.got.times();
        drop(rx);
        let last = *times.last().unwrap_or(&0);
        assert!((3_800..5_400).contains(&last), "{name}: 5 s of play reached {last} ms, so it is not paced or not looped");
        assert!(times.windows(2).all(|w| w[0] <= w[1]), "{name}: time went backwards across the loop");
        assert!(times.len() >= 95, "{name}: {} frames in 5 s", times.len());
    }
}

fn tag(kind: TagKind, ms: u32, key: bool, header: bool) -> MediaTag {
    let body: &[u8] = match kind {
        TagKind::Video => if key || header { &[0x17, u8::from(!header), 0, 0, 0] } else { &[0x27, 1, 0, 0, 0] },
        _ => &[0xAF, u8::from(!header), 0x11, 0x90],
    };
    MediaTag { kind, timestamp_ms: ms, keyframe: key, sequence_header: header, payload: Arc::from(body) }
}

/// A channel stream is read straight off the hub, and a second publisher
/// after the first leaves carries on the same timeline.
#[test]
fn a_channel_stream_is_read_off_the_hub_across_publishers() {
    let hub = Hub::new();
    let ctx = Context { hub: Some(hub.clone()) };
    let rx = start(json!("channel:church/main"), &ctx);
    // Let the reader subscribe first, or it joins at the second keyframe.
    std::thread::sleep(Duration::from_millis(500));
    for round in 0..2 {
        let p = hub.publish("church", "main", "test", None).expect("nobody else publishes");
        p.push(tag(TagKind::Video, 0, true, true));
        for i in 0..30 {
            p.push(tag(TagKind::Video, i * 40, i % 10 == 0, false));
            std::thread::sleep(Duration::from_millis(5));
        }
        let want = 30 * (round + 1);
        assert!(eventually(5, || rx.got.frames(TagKind::Video) >= want as usize), "round {round}: {}", rx.got.frames(TagKind::Video));
        drop(p);
        std::thread::sleep(Duration::from_millis(400));
    }
    let times = rx.got.times();
    assert!(times.windows(2).all(|w| w[0] <= w[1]), "the second publisher went back in time: {times:?}");
}

/// A main that never arrives hands over to its backup, a looped file.
#[test]
fn a_backup_takes_over_from_a_main_that_is_silent() {
    if !which("ffmpeg") {
        return;
    }
    let clip = ts_clip(4);
    let spec = json!({"uri": "udp://127.0.0.1:19935", "params": {"stall_ms": 1000},
                      "backup": format!("file://{}", path(&clip))});
    let rx = start(spec, &Context::default());
    assert!(eventually(10, || rx.got.keyframes() >= 2), "nothing from the backup");
    let s = rx.got.last();
    assert!(s.error.as_deref().unwrap_or("").contains("backup"), "{s:?}");
}
