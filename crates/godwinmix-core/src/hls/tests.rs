//! The packager against real GStreamer: a ladder made from a `videotestsrc`
//! with a scaler and an encoder per rung, keyframes forced every segment on
//! all of them, and AAC beside it. No planner, no mixer.

use super::ladder::{self, Rung};
use super::ring::Part;
use super::stream::Stream;
use super::track::{Track, TrackKind};
use super::{attach, HlsParams, Input};
use crate::gstutil::make;
use bytes::Bytes;
use gstreamer as gst;
use gstreamer::prelude::*;
use std::sync::Arc;
use std::time::{Duration, Instant};

fn small_ladder() -> Vec<Rung> {
    vec![
        Rung::new("360p", 640, 360, 800),
        Rung::new("270p", 480, 270, 500),
        Rung::new("180p", 320, 180, 300),
        Rung::new("144p", 256, 144, 200),
    ]
}

/// A live test picture and tone, laddered and packaged into `stream`.
fn running(stream: &Arc<Stream>, rungs: &[Rung]) -> gst::Pipeline {
    gst::init().unwrap();
    let pipeline = gst::Pipeline::new();
    let src = make("videotestsrc", "v").unwrap();
    src.set_property("is-live", true);
    let caps = gst::Caps::builder("video/x-raw").field("width", 640).field("height", 360).field("framerate", gst::Fraction::new(30, 1)).build();
    let filter = crate::gstutil::capsfilter("vcaps", &caps).unwrap();
    let asrc = make("audiotestsrc", "a").unwrap();
    asrc.set_property("is-live", true);
    let aconv = make("audioconvert", "aconv").unwrap();
    let aenc = make("avenc_aac", "aenc").unwrap();
    pipeline.add_many([&src, &filter, &asrc, &aconv, &aenc]).unwrap();
    src.link(&filter).unwrap();
    gst::Element::link_many([&asrc, &aconv, &aenc]).unwrap();
    let raw = filter.static_pad("src").unwrap();
    for (rung, pad) in ladder::encode(&pipeline, &raw, rungs, stream.params.segment_ms, "t").unwrap() {
        attach(&pipeline, stream, Input { id: &rung.id, kind: TrackKind::Video, pad: &pad, declared_kbps: rung.kbps }).unwrap();
    }
    let apad = aenc.static_pad("src").unwrap();
    attach(&pipeline, stream, Input { id: "audio", kind: TrackKind::Audio, pad: &apad, declared_kbps: 128 }).unwrap();
    pipeline.set_state(gst::State::Playing).unwrap();
    pipeline
}

fn wait_until(limit: Duration, mut done: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    while start.elapsed() < limit {
        if done() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn stop(pipeline: gst::Pipeline) {
    let _ = pipeline.set_state(gst::State::Null);
}

#[test]
fn a_four_rung_ladder_is_packaged_with_aligned_segments_and_parts() {
    if !crate::probe::have_or_skip("cmafmux") {
        return;
    }
    // A third of a second at 30 fps is the part that does not divide into
    // nanoseconds, which is what once made two second segments 2.67 s long.
    let params = HlsParams { segment_ms: 1000, part_ms: 333, window_s: 6 };
    let stream = Arc::new(Stream::new("ladder", params, "k".repeat(24).as_str()));
    let pipeline = running(&stream, &small_ladder());
    let whole = |t: &Arc<Track>| t.view().segments.iter().filter(|s| s.complete).count();
    let ok = wait_until(Duration::from_secs(15), || stream.tracks().iter().all(|t| whole(t) >= 3));
    let tracks = stream.tracks();
    stop(pipeline);
    assert!(ok, "every rung should have three whole segments: {}", stream.status());
    let videos: Vec<_> = tracks.iter().filter(|t| t.kind == TrackKind::Video).collect();
    assert_eq!(videos.len(), 4);
    // Keyframes on the same frames on every rung: the same segment numbers,
    // the same durations to the frame.
    let shape = |t: &Arc<Track>| -> Vec<(u64, u64)> {
        t.view().segments.iter().filter(|s| s.complete).map(|s| (s.msn, s.duration_ns / 33_000_000)).collect()
    };
    let first = shape(videos[0]);
    for v in &videos[1..] {
        let other = shape(v);
        let common: Vec<_> = first.iter().filter(|s| other.contains(s)).collect();
        assert!(common.len() >= 2, "{} and {} share no segments: {first:?} {other:?}", videos[0].id, v.id);
    }
    for v in &videos {
        let view = v.view();
        // Every whole segment after the first is one second to the frame:
        // the muxer cut at every forced keyframe.
        for s in view.segments.iter().filter(|s| s.complete).skip(1) {
            assert!((966_000_000..=1_034_000_000).contains(&s.duration_ns), "{} segment {}: {} ns", v.id, s.msn, s.duration_ns);
            assert_eq!(s.parts.len(), 3, "{} segment {} has {} parts", v.id, s.msn, s.parts.len());
        }
        let s = view.segments.iter().find(|s| s.complete).unwrap();
        assert!(s.parts[0].1, "the first part of a segment starts with a keyframe");
        assert!(v.info().codecs.starts_with("avc1."), "{:?}", v.info());
        assert!(v.init(0).is_some_and(|b| &b[4..8] == b"ftyp"));
    }
    let master = stream.master();
    assert_eq!(master.matches("#EXT-X-STREAM-INF").count(), 4, "{master}");
    assert!(master.contains("RESOLUTION=640x360") && master.contains("mp4a.40.2"), "{master}");
    assert!(master.contains("FRAME-RATE=30.000"), "{master}");
}

#[test]
fn a_part_is_served_within_a_part_of_its_keyframe() {
    if !crate::probe::have_or_skip("cmafmux") {
        return;
    }
    let params = HlsParams { segment_ms: 1000, part_ms: 250, window_s: 6 };
    let stream = Arc::new(Stream::new("edge", params, "k".repeat(24).as_str()));
    let pipeline = running(&stream, &small_ladder()[..1]);
    let track = stream.track("360p").unwrap();
    // Wait for a segment to open, then time how long its first part took
    // to arrive after the keyframe's wall clock time.
    let mut rx = track.watch();
    let mut lags = Vec::new();
    let deadline = Instant::now() + Duration::from_secs(12);
    let mut seen = None;
    while lags.len() < 4 && Instant::now() < deadline {
        if let Some((msn, parts)) = rx.borrow_and_update().open {
            if parts > 0 && seen != Some(msn) {
                seen = Some(msn);
                let view = track.view();
                let s = view.segments.iter().find(|s| s.msn == msn).unwrap();
                let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
                lags.push(now - s.pdt_ms);
            }
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    stop(pipeline);
    eprintln!("keyframe to first part served, ms: {lags:?}");
    assert!(lags.len() >= 3, "segments did not open: {lags:?}");
    for lag in &lags[1..] {
        assert!(*lag < 1000, "a first part took {lag} ms after its keyframe");
    }
}

#[test]
fn a_plain_segment_is_served_as_soon_as_it_is_whole() {
    if !crate::probe::have_or_skip("cmafmux") {
        return;
    }
    let params = HlsParams { segment_ms: 1000, part_ms: 0, window_s: 6 };
    let stream = Arc::new(Stream::new("plain", params, "k".repeat(24).as_str()));
    let pipeline = running(&stream, &small_ladder()[..1]);
    let track = stream.track("360p").unwrap();
    let rx = track.watch();
    let mut late = Vec::new();
    let mut seen = None;
    let deadline = Instant::now() + Duration::from_secs(12);
    while late.len() < 4 && Instant::now() < deadline {
        let done = rx.borrow().complete;
        if done.is_some() && done != seen {
            seen = done;
            let view = track.view();
            let s = view.segments.iter().find(|s| Some(s.msn) == done).unwrap();
            let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
            // How long after its last frame was made the segment could be fetched.
            late.push(now - s.pdt_ms - (s.duration_ns / 1_000_000) as i64);
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    stop(pipeline);
    eprintln!("segment end to segment served, ms: {late:?}");
    assert!(late.len() >= 3, "segments did not close: {late:?}");
    for l in &late[1..] {
        assert!(*l < 500, "a segment was served {l} ms after its last frame");
    }
}

#[test]
fn a_rung_leaves_and_the_others_carry_on() {
    if !crate::probe::have_or_skip("cmafmux") {
        return;
    }
    let params = HlsParams { segment_ms: 1000, part_ms: 0, window_s: 6 };
    let stream = Arc::new(Stream::new("leave", params, "k".repeat(24).as_str()));
    gst::init().unwrap();
    let pipeline = gst::Pipeline::new();
    let src = make("videotestsrc", "v").unwrap();
    src.set_property("is-live", true);
    pipeline.add(&src).unwrap();
    let pads = ladder::encode(&pipeline, &src.static_pad("src").unwrap(), &small_ladder()[2..], 1000, "t").unwrap();
    let mut attached: Vec<_> = pads
        .iter()
        .map(|(r, p)| attach(&pipeline, &stream, Input { id: &r.id, kind: TrackKind::Video, pad: p, declared_kbps: 0 }).unwrap())
        .collect();
    pipeline.set_state(gst::State::Playing).unwrap();
    assert!(wait_until(Duration::from_secs(10), || stream.ready()));
    let staying = stream.track("180p").unwrap();
    let before = staying.position().complete.unwrap();
    attached.pop().unwrap().detach(&pipeline, &stream);
    assert!(stream.track("144p").is_none());
    let moved = wait_until(Duration::from_secs(5), || staying.position().complete.unwrap() >= before + 2);
    stop(pipeline);
    assert!(moved, "the rung left behind stopped at {before}");
}

#[test]
fn a_slow_reader_holds_bytes_not_the_ring() {
    let track = Track::new("r", TrackKind::Video, HlsParams { window_s: 6, ..HlsParams::default() }, 0);
    track.set_init(Bytes::from_static(b"\0\0\0\x08ftyp"));
    let part = |n: u8| Part { bytes: Bytes::from(vec![n; 64 * 1024]), duration_ns: 2_000_000_000, independent: true };
    track.begin(0, 0, None);
    track.push_part(part(0));
    track.close();
    // A viewer that took segment 0 and then stopped reading.
    let held = track.segment(0).unwrap();
    let start = Instant::now();
    for i in 1..200u64 {
        track.begin(i * 2_000_000_000, 0, None);
        track.push_part(part(i as u8));
        track.close();
    }
    assert!(start.elapsed() < Duration::from_millis(500), "the packager waited on a reader");
    assert!(track.segment(0).is_none(), "segment 0 left the ring");
    assert_eq!(held[0][0], 0, "the reader still has its bytes");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blocking_reload_wakes_when_its_part_arrives() {
    let track = Arc::new(Track::new("r", TrackKind::Video, HlsParams { part_ms: 250, ..HlsParams::default() }, 0));
    track.set_init(Bytes::from_static(b"\0\0\0\x08ftyp"));
    let mut rx = track.watch();
    let waiter = tokio::spawn(async move {
        let started = Instant::now();
        rx.wait_for(|p| p.reached(7, Some(2))).await.unwrap();
        started.elapsed()
    });
    let producer = track.clone();
    std::thread::spawn(move || {
        let part = || Part { bytes: Bytes::from_static(b"p"), duration_ns: 250_000_000, independent: false };
        producer.begin(7 * 2_000_000_000, 0, None);
        for _ in 0..3 {
            std::thread::sleep(Duration::from_millis(100));
            producer.push_part(part());
        }
    });
    let waited = tokio::time::timeout(Duration::from_secs(3), waiter).await.expect("never woke").unwrap();
    assert!(waited >= Duration::from_millis(250), "woke before its part: {waited:?}");
}
