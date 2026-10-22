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
}
