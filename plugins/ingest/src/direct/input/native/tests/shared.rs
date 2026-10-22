//! Two shows on one feed share one socket, each gets its own program, and
//! the socket closes when the last one leaves. Ports 19941 to 19949.

use std::time::Duration;

use serde_json::json;

use super::super::registry;
use crate::direct::input::tests::{eventually, one_at_a_time, spawn, start, which};
use crate::direct::input::Context;
use crate::media_tag::TagKind;

/// Open groups on `port`.
fn on(port: u16) -> usize {
    registry::snapshot().iter().filter(|g| g["address"].as_str().is_some_and(|a| a.ends_with(&format!(":{port}")))).count()
}

#[test]
fn two_programs_of_one_feed_share_its_socket_and_it_closes_after_the_last() {
    let _one = one_at_a_time();
    if !which("ffmpeg") {
        return;
    }
    let port = 19941;
    let _tx = spawn("ffmpeg", &["-loglevel", "error", "-re",
        "-f", "lavfi", "-i", "testsrc2=size=320x240:rate=25", "-f", "lavfi", "-i", "sine=frequency=440",
        "-f", "lavfi", "-i", "testsrc2=size=352x288:rate=25", "-f", "lavfi", "-i", "sine=frequency=880",
        "-map", "0:v", "-map", "1:a", "-map", "2:v", "-map", "3:a", "-t", "20",
        "-c:v", "libx264", "-preset", "ultrafast", "-g", "25", "-c:a", "aac",
        "-program", "program_num=1:title=Sport:st=0:st=1", "-program", "program_num=2:title=News:st=2:st=3",
        "-f", "mpegts", &format!("udp://127.0.0.1:{port}?pkt_size=1316")]);
    let one = start(json!({"uri": format!("udp://127.0.0.1:{port}"), "program": 1}), &Context::default());
    let two = start(json!({"uri": format!("udp://127.0.0.1:{port}"), "program": 2}), &Context::default());
    let both = || one.got.keyframes() >= 3 && two.got.keyframes() >= 3 && two.got.last().width > 0 && one.got.last().width > 0;
    assert!(eventually(15, both), "{:?} and {:?}", one.got.last(), two.got.last());
    assert_eq!(on(port), 1, "one socket for both shows");
    assert_eq!((one.got.last().width, two.got.last().width), (320, 352), "each show its own program's picture");
    assert!(one.got.frames(TagKind::Audio) > 20 && two.got.frames(TagKind::Audio) > 20);
    let s = two.got.last();
    assert_eq!(s.program, Some(2));
    assert_eq!(s.programs.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Sport", "News"]);
    assert_eq!(s.tracks.iter().map(|t| t.kind).collect::<Vec<_>>(), ["video", "audio"], "{:?}", s.tracks);
    drop(one);
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(on(port), 1, "still open for the second show");
    let before = two.got.frames(TagKind::Video);
    assert!(eventually(5, || two.got.frames(TagKind::Video) > before + 25), "the second show carried on alone");
    drop(two);
    assert!(eventually(3, || on(port) == 0), "the socket closed after the last show left");
}
