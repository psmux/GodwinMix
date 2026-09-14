//! The claim this crate makes about its own Matroska writer, checked against a
//! real demuxer: `decodebin` opens it, the caps are the canvas caps, and every
//! frame written comes out.
//!
//! Only built with `--features gst`, because it needs GStreamer to read the
//! stream. The writer under test needs nothing.

#![cfg(feature = "gst")]

use std::io::Write;

use godwinmix_sdk::media::{AudioTrack, MatroskaWriter, VideoFormat, VideoTrack};
use godwinmix_sdk::wire::Canvas;

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::AppSink;

const CANVAS: Canvas = Canvas {
    width: 320,
    height: 180,
    fps: 30,
};

/// Eight bars, the same values the example draws.
fn colour_bars(canvas: Canvas) -> Vec<u8> {
    const BARS: [[u8; 3]; 8] = [
        [235, 128, 128],
        [210, 16, 146],
        [170, 166, 16],
        [145, 54, 34],
        [106, 202, 222],
        [81, 90, 240],
        [41, 240, 110],
        [16, 128, 128],
    ];
    let width = canvas.width as usize;
    let height = canvas.height as usize;
    let cw = width.div_ceil(2);
    let ch = height.div_ceil(2);
    let mut frame = vec![0u8; canvas.i420_frame_bytes()];
    let bar_of = |x: usize| (x * 8 / width).min(7);
    for row in 0..height {
        for x in 0..width {
            frame[row * width + x] = BARS[bar_of(x)][0];
        }
    }
    let u_at = width * height;
    let v_at = u_at + cw * ch;
    for row in 0..ch {
        for x in 0..cw {
            let bar = BARS[bar_of(x * 2)];
            frame[u_at + row * cw + x] = bar[1];
            frame[v_at + row * cw + x] = bar[2];
        }
    }
    frame
}

/// Write one second of bars into a file with the pure Rust writer.
fn write_one_second(path: &std::path::Path, with_audio: bool) -> usize {
    let file = std::fs::File::create(path).expect("could not create the test file");
    let mut writer = MatroskaWriter::new(
        std::io::BufWriter::new(file),
        Some(VideoTrack {
            width: CANVAS.width,
            height: CANVAS.height,
            fps: CANVAS.fps,
            format: VideoFormat::I420,
        }),
        with_audio.then(AudioTrack::default),
    );
    let frame = colour_bars(CANVAS);
    let silence = vec![0u8; 480 * 2 * 4];
    let frames = CANVAS.fps as usize;
    for i in 0..frames {
        let pts = i as u64 * CANVAS.frame_duration_ns();
        writer.write_video(pts, &frame, true).expect("video");
        if with_audio {
            // Three 10 ms buffers per frame at 30 fps, near enough.
            for j in 0..3 {
                let apts = pts + j * 10_000_000;
                writer.write_audio(apts, &silence).expect("audio");
            }
        }
    }
    writer.finish().expect("finish").flush().expect("flush");
    frames
}

/// Pull every buffer out of one decoded stream and report the caps.
fn decode(path: &std::path::Path, media: &str) -> (gst::Caps, usize) {
    gst::init().expect("GStreamer would not start");
    let description = format!(
        "filesrc location={} ! decodebin name=d ! queue ! {media}convert ! \
         appsink name=out sync=false max-buffers=0",
        path.display()
    );
    let pipeline = gst::parse::launch(&description)
        .expect("could not build the reading pipeline")
        .downcast::<gst::Pipeline>()
        .expect("not a pipeline");
    let sink = pipeline
        .by_name("out")
        .expect("no appsink")
        .downcast::<AppSink>()
        .expect("not an appsink");
    pipeline.set_state(gst::State::Playing).expect("play");

    let mut count = 0usize;
    let mut caps = None;
    while let Ok(sample) = sink.pull_sample() {
        if caps.is_none() {
            caps = sample.caps().map(|c| c.to_owned());
        }
        count += 1;
    }
    pipeline.set_state(gst::State::Null).expect("null");
    (caps.expect("the stream produced no caps"), count)
}

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("godwinmix-sdk-tests");
    std::fs::create_dir_all(&dir).expect("could not make the temp directory");
    dir.join(name)
}

#[test]
fn decodebin_opens_the_pure_rust_stream_at_canvas_caps() {
    let path = temp("one-second-video.mkv");
    let written = write_one_second(&path, false);
    let (caps, decoded) = decode(&path, "video");

    let s = caps.structure(0).expect("no structure");
    assert_eq!(s.name(), "video/x-raw");
    assert_eq!(s.get::<i32>("width").unwrap(), CANVAS.width as i32);
    assert_eq!(s.get::<i32>("height").unwrap(), CANVAS.height as i32);
    assert_eq!(s.get::<String>("format").unwrap(), "I420");
    assert_eq!(
        s.get::<gst::Fraction>("framerate").unwrap(),
        gst::Fraction::new(CANVAS.fps as i32, 1)
    );
    assert_eq!(
        decoded, written,
        "wrote {written} frames and read back {decoded}"
    );
    let _ = std::fs::remove_file(&path);
}

#[test]
fn a_stream_with_sound_carries_both_tracks() {
    let path = temp("one-second-both.mkv");
    write_one_second(&path, true);
    let (video_caps, video_frames) = decode(&path, "video");
    assert_eq!(
        video_caps.structure(0).unwrap().get::<String>("format").unwrap(),
        "I420"
    );
    assert_eq!(video_frames, CANVAS.fps as usize);

    let (audio_caps, audio_buffers) = decode(&path, "audio");
    let s = audio_caps.structure(0).unwrap();
    assert_eq!(s.name(), "audio/x-raw");
    assert_eq!(s.get::<i32>("rate").unwrap(), 48_000);
    assert_eq!(s.get::<i32>("channels").unwrap(), 2);
    assert!(
        audio_buffers >= 90,
        "expected about 90 buffers of 10 ms, got {audio_buffers}"
    );
    let _ = std::fs::remove_file(&path);
}
