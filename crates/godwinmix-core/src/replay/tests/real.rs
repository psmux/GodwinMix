//! Real encoders on a real test pattern: what the ring holds, that a slow
//! encoder costs the source nothing, and that a save is a playable MP4.

use super::super::angle::{self, EncodeWith};
use super::super::branch::{Branch, Side};
use super::super::ring::Ring;
use super::SEC;
use crate::config::Config;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub fn encode_with() -> EncodeWith {
    let cfg: Config = toml::from_str("").unwrap();
    let sel = crate::catalogue::select(&cfg, None).unwrap();
    let vars = crate::catalogue::apply::Vars {
        video_bitrate_kbps: 2000,
        audio_bitrate_kbps: 128,
        keyframe_secs: 2,
        keyframe_frames: 60,
        fps: 30,
        cpu_count: crate::catalogue::apply::cpu_count(),
    };
    EncodeWith { video: sel.video_encode, audio: sel.audio_encode, vars }.for_replay(0)
}

/// A live 640x360 pattern and tone with a tee each, like a source's.
pub struct Pattern {
    pub pipeline: gst::Pipeline,
    pub vtee: gst::Element,
    pub atee: gst::Element,
    /// Frames the other branch of the picture tee received.
    pub seen: Arc<AtomicU64>,
}

pub fn pattern() -> Pattern {
    let pipeline = gst::parse::launch(
        "videotestsrc is-live=true pattern=ball ! video/x-raw,width=640,height=360,framerate=30/1,format=I420 \
         ! tee name=vt allow-not-linked=true vt. ! queue ! fakesink name=pgm sync=true async=false \
         audiotestsrc is-live=true samplesperbuffer=480 ! audio/x-raw,rate=48000,channels=2,format=S16LE \
         ! tee name=at allow-not-linked=true",
    )
    .unwrap()
    .downcast::<gst::Pipeline>()
    .unwrap();
    let vtee = pipeline.by_name("vt").unwrap();
    let atee = pipeline.by_name("at").unwrap();
    let seen = Arc::new(AtomicU64::new(0));
    let count = seen.clone();
    let sink = pipeline.by_name("pgm").unwrap();
    sink.static_pad("sink").unwrap().add_probe(gst::PadProbeType::BUFFER, move |_, _| {
        count.fetch_add(1, Ordering::Relaxed);
        gst::PadProbeReturn::Ok
    });
    pipeline.set_state(gst::State::Playing).unwrap();
    Pattern { pipeline, vtee, atee, seen }
}

#[test]
fn a_real_angle_holds_whole_seconds_of_its_source() {
    gst::init().unwrap();
    let p = pattern();
    let ring = Arc::new(Mutex::new(Ring::new(2)));
    let b = angle::source(&p.pipeline, &p.vtee, Some(&p.atee), &ring, &encode_with(), "t-real").unwrap();
    std::thread::sleep(Duration::from_millis(4500));
    let (held, gops, bytes, audio) = {
        let r = ring.lock();
        (r.held(), r.gop_count(), r.bytes(), r.everything().audio.len())
    };
    drop(b);
    p.pipeline.set_state(gst::State::Null).unwrap();
    assert!(held >= 2 * SEC - SEC / 15 && held <= 3 * SEC + SEC / 15, "two seconds and at most one GOP more: {held}");
    assert!((2..=3).contains(&gops), "a keyframe every second: {gops} GOPs");
    assert!(bytes > 0 && bytes < 3 * 2_000_000 / 8 * 2, "about the bitrate times the seconds: {bytes} bytes");
    assert!(audio > 50, "the sound is kept beside the picture: {audio} frames");
}

#[test]
fn a_slow_encoder_drops_its_own_frames_and_the_source_loses_none() {
    gst::init().unwrap();
    let p = pattern();
    let ring = Arc::new(Mutex::new(Ring::new(5)));
    // An encoder that takes 100 ms a frame: a third of real time.
    let slow = gst::ElementFactory::make("identity").property("sleep-time", 100_000u32).build().unwrap();
    let queue = crate::gstutil::queue_preview("t-slow-q").unwrap();
    let branch = Branch::attach(&p.pipeline, &p.vtee, queue, vec![slow], &ring, Side::Video, "t-slow").unwrap();
    let before = p.seen.load(Ordering::Relaxed);
    std::thread::sleep(Duration::from_secs(3));
    let seen = p.seen.load(Ordering::Relaxed) - before;
    let dropped = branch.dropped();
    drop(branch);
    p.pipeline.set_state(gst::State::Null).unwrap();
    assert!(seen >= 85, "the source's own branch got every frame of three seconds: {seen}");
    assert!(dropped >= 40, "the slow encoder's queue dropped what it could not take: {dropped}");
}

#[test]
fn a_save_is_an_mp4_of_the_clip_s_length() {
    gst::init().unwrap();
    let p = pattern();
    let ring = Arc::new(Mutex::new(Ring::new(10)));
    let b = angle::source(&p.pipeline, &p.vtee, Some(&p.atee), &ring, &encode_with(), "t-save").unwrap();
    std::thread::sleep(Duration::from_millis(4300));
    drop(b);
    p.pipeline.set_state(gst::State::Null).unwrap();
    let clip = ring.lock().everything();
    let path = std::env::temp_dir().join(format!("gmx-replay-save-{}.mp4", std::process::id()));
    let written = super::super::save::write(&clip, &path).unwrap();
    let info = gstreamer_pbutils::Discoverer::new(gst::ClockTime::from_seconds(10)).unwrap().discover_uri(&crate::input::file_uri(&path)).unwrap();
    let length = info.duration().unwrap().nseconds();
    let want = clip.to - clip.video[0].frame.at;
    assert!(length.abs_diff(want) < SEC / 10, "the file runs {length} ns for a clip of {want} ns");
    assert!(length > 3 * SEC, "about four seconds were recorded: {length}");
    assert_eq!(info.video_streams().len(), 1);
    assert_eq!(info.audio_streams().len(), 1, "the sound is saved too");
    assert!(written.bytes > 10_000, "{} bytes", written.bytes);
    let _ = std::fs::remove_file(&path);
}
