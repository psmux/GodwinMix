//! The ring and the marks with made up frames, then real encoders.

use super::marks::Marks;
use super::ring::{Frame, Ring};
use super::Refusal;
use godwinmix_protocol::replay::{MarkPoint, MarkRequest};
use gstreamer as gst;

mod real;

const SEC: u64 = 1_000_000_000;

fn caps() -> gst::Caps {
    gst::Caps::builder("video/x-h264").field("stream-format", "byte-stream").build()
}

/// A frame of `size` bytes at `at`, a keyframe when `key`.
fn frame(at: u64, key: bool, size: usize) -> Frame {
    Frame { at, dts: None, duration: SEC / 30, key, buffer: gst::Buffer::with_size(size).unwrap() }
}

/// `secs` seconds of 30 fps with a keyframe every `gop` frames.
fn fill(ring: &mut Ring, secs: u64, gop: u64, size: usize) {
    let caps = caps();
    for i in 0..secs * 30 {
        ring.push_video(frame(i * SEC / 30, i % gop == 0, size), &caps);
    }
}

#[test]
fn the_ring_holds_its_depth_and_never_more_than_one_gop_over() {
    gst::init().unwrap();
    let mut ring = Ring::new(10);
    fill(&mut ring, 60, 30, 1000);
    let held = ring.held();
    assert!(held + 1000 >= 10 * SEC, "it keeps at least what it says: {held}");
    assert!(held < 11 * SEC, "and less than one GOP more: {held}");
    let most = 10 * 30 * 1000 + ring.largest_gop_bytes() + 1000;
    assert!(ring.bytes() <= most, "memory is depth times bitrate plus a GOP: {} > {most}", ring.bytes());
    assert!((10..=11).contains(&ring.gop_count()), "{} GOPs", ring.gop_count());
}

#[test]
fn pictures_before_the_first_keyframe_are_not_kept() {
    gst::init().unwrap();
    let mut ring = Ring::new(5);
    let caps = caps();
    ring.push_video(frame(0, false, 10), &caps);
    ring.push_video(frame(SEC / 30, false, 10), &caps);
    assert_eq!(ring.gop_count(), 0);
    assert_eq!(ring.orphans, 2);
    ring.push_video(frame(2 * SEC / 30, true, 10), &caps);
    assert_eq!(ring.gop_count(), 1);
}

#[test]
fn a_clip_starts_at_the_keyframe_before_its_in_point_and_stops_at_its_out() {
    gst::init().unwrap();
    let mut ring = Ring::new(30);
    fill(&mut ring, 20, 30, 100);
    let clip = ring.clip(5 * SEC + SEC / 2, 8 * SEC);
    assert_eq!(clip.video[0].frame.at, 5 * SEC, "decoding starts from the keyframe");
    assert!(clip.video[0].frame.key && clip.video[0].caps.is_some());
    assert!(clip.video.last().unwrap().frame.at < 8 * SEC);
    assert_eq!(clip.video.len(), 90, "three seconds from 5.0 at 30 fps");
    assert_eq!(clip.from, 5 * SEC + SEC / 2, "and remembers where to start showing");
    assert_eq!(clip.key_before(7 * SEC + 10), 60);
}

#[test]
fn the_sound_older_than_the_oldest_picture_goes_with_it() {
    gst::init().unwrap();
    let mut ring = Ring::new(2);
    let acaps = gst::Caps::builder("audio/mpeg").field("mpegversion", 4i32).build();
    for i in 0..200u64 {
        ring.push_audio(Frame { at: i * SEC / 50, dts: None, duration: SEC / 50, key: true, buffer: gst::Buffer::with_size(10).unwrap() }, &acaps);
    }
    fill(&mut ring, 4, 30, 10);
    let (oldest, _) = ring.span().unwrap();
    let clip = ring.everything();
    assert!(clip.audio.iter().all(|f| f.at >= oldest));
}

fn mark(point: Option<MarkPoint>, seconds: Option<f64>) -> MarkRequest {
    MarkRequest { point, seconds, ..MarkRequest::default() }
}

#[test]
fn marks_open_and_close_events_with_slugs_that_are_never_reused() {
    let mut m = Marks::default();
    let e = m.mark(&mark(Some(MarkPoint::In), Some(2.0)), 10_000).unwrap();
    assert_eq!((e.id.as_str(), e.in_ms, e.out_ms), ("replay-1", 8_000, None));
    let e = m.mark(&mark(Some(MarkPoint::Out), None), 12_500).unwrap();
    assert_eq!((e.in_ms, e.out_ms), (8_000, Some(12_500)));
    let last = m.mark(&mark(None, Some(5.0)), 20_000).unwrap();
    assert_eq!((last.id.as_str(), last.in_ms, last.out_ms), ("replay-2", 15_000, Some(20_000)));
    m.delete("replay-1").unwrap();
    let next = m.mark(&mark(None, Some(1.0)), 30_000).unwrap();
    assert_eq!(next.id, "replay-3");
    m.rename("replay-3", "Goal").unwrap();
    assert_eq!(m.get("replay-3").unwrap().name, "Goal");
}

#[test]
fn a_mark_that_cannot_be_made_says_what_to_do() {
    let mut m = Marks::default();
    let out = m.mark(&mark(Some(MarkPoint::Out), None), 1000).unwrap_err();
    assert!(matches!(out, Refusal::NotInState(ref s) if s.contains("Mark an in point first")), "{out}");
    assert!(matches!(m.mark(&mark(None, None), 1000), Err(Refusal::Invalid { field: "point", .. })));
    assert!(matches!(m.mark(&mark(None, Some(-1.0)), 1000), Err(Refusal::Invalid { field: "seconds", .. })));
    let gone = m.delete("replay-9").unwrap_err().to_string();
    assert!(gone.contains("replay-9") && gone.contains("none"), "{gone}");
}
