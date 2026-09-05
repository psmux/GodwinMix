//! Raw video and audio out of the sidecar, into the mixer, losslessly.
//!
//! The browser hands us BGRA frames and float PCM. This module puts them into a
//! Matroska stream on a file descriptor (stdout, for an `exec:` source) with no
//! encoder in the path. The mixer's `decodebin` reads it straight back out as
//! raw video and audio, which was checked before this was written: `matroskamux`
//! carries I420 (not BGRA, so frames are converted) and F32LE, and
//! `decodebin` round-trips both with zero errors.
//!
//! Two timing decisions live here:
//!
//! * **Fixed cadence.** A windowless browser only paints when something on the
//!   page changes, so a mostly static page produces few frames. The mixer wants
//!   a steady rate. A pacing thread re-sends the most recent frame at exactly
//!   the target frame rate, so what leaves is constant-rate regardless of how
//!   often the page repaints.
//! * **One clock for both.** Video and audio are stamped against the same
//!   clock, started when the muxer is built. A frame is stamped with the
//!   pacer tick that sends it. An audio packet is stamped with the
//!   presentation time Chromium attaches to it (wall clock, mapped onto ours),
//!   which is what makes a stream that starts seconds after load land in the
//!   right place; the sample count then carries the timeline gaplessly and
//!   is re-anchored to Chromium's time when the two part by more than a
//!   couple of frames, as they do after a dropout.
//! * **Measured, not assumed.** Stamped that way, a beep lands within a frame
//!   of its flash on the test pages (`test/sync.html`, a WebAudio tone with
//!   a flash scheduled on the same audio clock, and a `<video>` with the
//!   pattern baked in), on Linux and on macOS. The earlier scheme, arrival
//!   time for audio and the previous tick for video, put audio 60 ms late.
//!   `--audio-offset-ms` exists for a page or platform where the measurement
//!   says otherwise; `dev/measure-sync.py` is how to take it.

use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Milliseconds added to every audio timestamp; negative pulls audio earlier.
/// Zero is what the measurement in the module notes supports.
pub const DEFAULT_AUDIO_OFFSET_MS: i64 = 0;

pub struct Muxer {
    pipeline: gst::Pipeline,
    video: gst_app::AppSrc,
    audio: gst_app::AppSrc,
    fps: i32,
    frame_bytes: usize,
    latest: Arc<Mutex<Option<Vec<u8>>>>,
    /// Pts of the first packet of the current audio stream and the number of
    /// frames delivered since; `None` until the next packet sets a new anchor.
    audio_clock: Mutex<Option<(u64, u64)>>,
    start: std::time::Instant,
    /// Unix time of `start`, in nanoseconds, to map Chromium's timestamps.
    start_unix_ns: i64,
    audio_offset_ns: i64,
    channels: i32,
    rate: i32,
}

/// How far the sample count may run from the wall clock before audio is
/// re-anchored. Two video frames at 30 fps.
const AUDIO_DRIFT_TOLERANCE_NS: u64 = 66_000_000;

impl Muxer {
    /// Build the output pipeline. `fd` is where the stream goes; 1 for stdout.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: i32,
        height: i32,
        fps: i32,
        channels: i32,
        rate: i32,
        fd: i32,
        audio_offset_ms: i64,
    ) -> Result<Arc<Self>, String> {
        gst::init().map_err(|e| e.to_string())?;

        let pipeline = gst::Pipeline::with_name("browser-out");
        let vcaps = gst::Caps::builder("video/x-raw")
            .field("format", "BGRA")
            .field("width", width)
            .field("height", height)
            .field("framerate", gst::Fraction::new(fps, 1))
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .build();
        let video = gst_app::AppSrc::builder()
            .name("video")
            .caps(&vcaps)
            .is_live(true)
            .format(gst::Format::Time)
            .build();

        let acaps = gst::Caps::builder("audio/x-raw")
            .field("format", "F32LE")
            .field("rate", rate)
            .field("channels", channels)
            .field("layout", "interleaved")
            .build();
        let audio = gst_app::AppSrc::builder()
            .name("audio")
            .caps(&acaps)
            .is_live(true)
            .format(gst::Format::Time)
            .build();

        let mk = |f: &str| gst::ElementFactory::make(f).build().map_err(|e| format!("{f}: {e}"));
        let vconv = mk("videoconvert")?;
        let i420 = mk("capsfilter")?;
        i420.set_property(
            "caps",
            gst::Caps::builder("video/x-raw").field("format", "I420").build(),
        );
        let vq = mk("queue")?;
        let aconv = mk("audioconvert")?;
        let aq = mk("queue")?;
        let mux = mk("matroskamux")?;
        // A pipe cannot be seeked, so the muxer must not try to rewrite headers.
        mux.set_property("streamable", true);
        let sink = mk("fdsink")?;
        sink.set_property("fd", fd);
        sink.set_property("sync", false);

        let velem: gst::Element = video.clone().upcast();
        let aelem: gst::Element = audio.clone().upcast();
        pipeline
            .add_many([&velem, &vconv, &i420, &vq, &aelem, &aconv, &aq, &mux, &sink])
            .map_err(|e| e.to_string())?;
        gst::Element::link_many([&velem, &vconv, &i420, &vq, &mux]).map_err(|e| e.to_string())?;
        gst::Element::link_many([&aelem, &aconv, &aq, &mux]).map_err(|e| e.to_string())?;
        mux.link(&sink).map_err(|e| e.to_string())?;

        pipeline.set_state(gst::State::Playing).map_err(|e| e.to_string())?;

        let m = Arc::new(Self {
            pipeline,
            video,
            audio,
            fps,
            frame_bytes: (width * height * 4) as usize,
            latest: Arc::new(Mutex::new(None)),
            audio_clock: Mutex::new(None),
            start: std::time::Instant::now(),
            start_unix_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as i64)
                .unwrap_or(0),
            audio_offset_ns: audio_offset_ms * 1_000_000,
            channels,
            rate,
        });
        m.clone().start_pacing();
        Ok(m)
    }

    /// The browser painted. Keep it; the pacing thread sends it.
    pub fn video_frame(&self, bgra: &[u8]) {
        if bgra.len() != self.frame_bytes {
            return;
        }
        let mut latest = self.latest.lock().unwrap();
        match latest.as_mut() {
            Some(buf) => buf.copy_from_slice(bgra),
            None => *latest = Some(bgra.to_vec()),
        }
    }

    /// The browser started (or restarted) an audio stream. The next packet
    /// sets a fresh anchor rather than continuing the old sample count.
    pub fn audio_stream_started(&self) {
        *self.audio_clock.lock().unwrap() = None;
    }

    /// Interleaved float PCM straight from the browser, with the presentation
    /// time Chromium gave it (milliseconds since the Unix epoch; 0 if none).
    pub fn audio_packet(&self, interleaved_f32le: &[u8], pts_unix_ms: i64) {
        let rate = self.rate.max(1) as u64;
        let frames = interleaved_f32le.len() as u64 / (4 * self.channels.max(1) as u64);
        let dur_ns = frames * 1_000_000_000 / rate;
        // Chromium's time for this packet on our clock, or, without one, the
        // interval that just ended.
        let theirs = if pts_unix_ms > 0 {
            pts_unix_ms * 1_000_000 - self.start_unix_ns
        } else {
            self.start.elapsed().as_nanos() as i64 - dur_ns as i64
        };
        let theirs = (theirs + self.audio_offset_ns).max(0) as u64;
        let mut clock = self.audio_clock.lock().unwrap();
        let pts_ns = match *clock {
            Some((base, n)) => {
                let predicted = base + n * 1_000_000_000 / rate;
                if predicted.abs_diff(theirs) > AUDIO_DRIFT_TOLERANCE_NS {
                    eprintln!(
                        "[browser] audio re-anchored: sample clock was {} ms from the browser's",
                        (predicted as i64 - theirs as i64) / 1_000_000
                    );
                    *clock = Some((theirs, frames));
                    theirs
                } else {
                    *clock = Some((base, n + frames));
                    predicted
                }
            }
            None => {
                *clock = Some((theirs, frames));
                theirs
            }
        };
        drop(clock);
        let pts = gst::ClockTime::from_nseconds(pts_ns);
        let dur = gst::ClockTime::from_nseconds(dur_ns);

        let mut buf = gst::Buffer::from_slice(interleaved_f32le.to_vec());
        {
            let b = buf.get_mut().unwrap();
            b.set_pts(pts);
            b.set_duration(dur);
        }
        let _ = self.audio.push_buffer(buf);
    }

    /// Emit the latest frame at exactly the target rate, whether or not the
    /// page repainted, so the mixer sees a constant frame rate.
    fn start_pacing(self: Arc<Self>) {
        let period = Duration::from_nanos(1_000_000_000 / self.fps.max(1) as u64);
        std::thread::Builder::new()
            .name("frame-pacer".into())
            .spawn(move || {
                let start = self.start;
                let mut n: u64 = 0;
                loop {
                    let due = start + period * (n as u32 + 1);
                    let now = std::time::Instant::now();
                    if due > now {
                        std::thread::sleep(due - now);
                    }
                    let frame = self.latest.lock().unwrap().clone();
                    if let Some(bytes) = frame {
                        // Stamped with this tick: the frame was painted at
                        // some point during the period that just ended.
                        let pts = gst::ClockTime::from_nseconds((n + 1) * period.as_nanos() as u64);
                        let mut buf = gst::Buffer::from_slice(bytes);
                        {
                            let b = buf.get_mut().unwrap();
                            b.set_pts(pts);
                            b.set_duration(gst::ClockTime::from_nseconds(period.as_nanos() as u64));
                        }
                        if self.video.push_buffer(buf).is_err() {
                            return;
                        }
                    }
                    n += 1;
                }
            })
            .expect("spawning frame pacer");
    }

    /// Block until the output pipeline fails, which is what happens when the
    /// reader of stdout goes away. The caller then stops the browser rather
    /// than rendering into a closed pipe forever.
    pub fn wait_for_failure(&self) -> String {
        let Some(bus) = self.pipeline.bus() else { return "no bus".into() };
        loop {
            let Some(msg) = bus.timed_pop_filtered(
                gst::ClockTime::NONE,
                &[gst::MessageType::Error, gst::MessageType::Eos],
            ) else {
                continue;
            };
            return match msg.view() {
                gst::MessageView::Error(e) => format!("{} ({:?})", e.error(), e.debug()),
                _ => "end of stream".into(),
            };
        }
    }

    pub fn finish(&self) {
        let _ = self.video.end_of_stream();
        let _ = self.audio.end_of_stream();
        // Give the muxer a moment to flush the tail before the pipe closes.
        std::thread::sleep(Duration::from_millis(300));
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}
