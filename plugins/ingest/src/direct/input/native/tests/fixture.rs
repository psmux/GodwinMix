//! A recorded feed, kept outside the repository, against the GStreamer
//! path: `GMX_TS_FIXTURE=/path/to/capture.ts` and `GMX_TS_PROGRAM=2`. The
//! one this was written for is H.264 field coded at 60 fields a second,
//! with no IDR at all: a decoder starts at an I slice behind a recovery
//! point SEI, and both paths must call those the keyframes.

use super::compare::assert_program;
use crate::direct::input::spec::Carry;

#[test]
fn a_recorded_feed_is_byte_for_byte_the_gstreamer_paths() {
    let Some(path) = std::env::var_os("GMX_TS_FIXTURE") else {
        eprintln!("skipping: set GMX_TS_FIXTURE to a recorded TS file");
        return;
    };
    let program = std::env::var("GMX_TS_PROGRAM").ok().and_then(|p| p.parse().ok());
    let carry = Carry { mpeg2: true, tracks: true };
    let sounds = std::env::var("GMX_TS_SOUNDS").ok().and_then(|p| p.parse().ok()).unwrap_or(1);
    let path = std::path::Path::new(&path);
    assert_program(path, "the recorded feed", carry, sounds, program);
    let tags = super::feed::tags_of(&std::fs::read(path).unwrap(), carry, program);
    let keys = tags.iter().filter(|t| t.keyframe && !t.sequence_header).count();
    assert!(keys > 0, "no keyframe in {} tags", tags.len());
    eprintln!("{} tags, {keys} keyframes", tags.len());
    if let (Some(p), Ok(_)) = (program, std::env::var("GMX_TS_FIELDS")) {
        let (all, fields) = super::feed::pictures(&std::fs::read(path).unwrap(), p);
        assert!(fields * 2 > all, "{fields} fields of {all} video tags");
    }
}

/// The recorded feed sent over UDP in real time: a field coded program
/// reports its frame rate, not its field rate, and says it is field coded.
/// `GMX_TS_SECONDS` is how long the recording is. Port 19943.
#[test]
fn a_recorded_field_coded_feed_reports_frames_not_fields() {
    let (Some(path), Ok(_)) = (std::env::var_os("GMX_TS_FIXTURE"), std::env::var("GMX_TS_FIELDS")) else {
        eprintln!("skipping: set GMX_TS_FIXTURE and GMX_TS_FIELDS for a field coded recording");
        return;
    };
    let program: u16 = std::env::var("GMX_TS_PROGRAM").ok().and_then(|p| p.parse().ok()).unwrap_or(1);
    let secs: f64 = std::env::var("GMX_TS_SECONDS").ok().and_then(|p| p.parse().ok()).unwrap_or(15.0);
    let ts = std::fs::read(path).unwrap();
    let rx = crate::direct::input::tests::start(serde_json::json!({"uri": "udp://127.0.0.1:19943", "program": program}), &crate::direct::input::Context::default());
    let sock = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let (started, per) = (std::time::Instant::now(), secs / (ts.len() as f64 / 1316.0));
    for (i, d) in ts.chunks(1316).enumerate() {
        let due = std::time::Duration::from_secs_f64(i as f64 * per);
        if let Some(wait) = due.checked_sub(started.elapsed()) {
            std::thread::sleep(wait);
        }
        let _ = sock.send_to(d, "127.0.0.1:19943");
    }
    let s = rx.got.last();
    assert!((28.0..32.0).contains(&s.fps), "{} fps, not 29.97 frames: {s:?}", s.fps);
    assert!(s.tracks.iter().any(|t| t.kind == "video" && t.field_coded), "{:?}", s.tracks);
}
