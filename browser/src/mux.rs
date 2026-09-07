//! Raw video and audio out of the sidecar, into the mixer, losslessly.
//!
//! The browser hands us BGRA frames and float PCM. This module puts them into a
//! Matroska stream on a file descriptor (stdout, for an `exec:` source) with no
//! encoder in the path. The mixer's `decodebin` reads it straight back out as
//! raw video and audio, which was checked before this was written: `matroskamux`
//! carries I420 (not BGRA, so frames are converted) and F32LE, and
//! `decodebin` round-trips both with zero errors.
//!
//! Transparent mode carries the alpha channel instead. `matroskamux` takes no
//! eight bit format with alpha except AYUV, so that is the format the frames
//! are converted to, and the premultiplied alpha Skia hands us is undone first.
//! See `unpremultiply_table`.
//!
//! One warning about reading that stream with something other than GStreamer.
//! GStreamer lays AYUV out as A, Y, U, V per pixel; the AYUV FOURCC Microsoft
//! defined, which is what FFmpeg matches the tag against, is V, U, Y, A. So
//! `ffprobe` sees the alpha and reports `vuya`, but `ffmpeg` decodes the four
//! bytes backwards: a fully transparent black pixel (0, 16, 128, 128) comes out
//! of `ffmpeg` as opaque-ish green. The mixer reads this with `decodebin`, which
//! agrees with `matroskamux`, so the round trip is correct; only FFmpeg is not
//! a way to check it.
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
    /// Present only in transparent mode. See `unpremultiply_table`.
    unpremultiply: Option<Vec<u8>>,
    /// Buffers dropped because the reader was behind. See `behind`.
    dropped: std::sync::atomic::AtomicU64,
}

/// rgb(255, 0, 254), the colour `detect-media.js` paints in a taken-over
/// video's box. Chosen to be nothing a page is likely to contain.
const KEY_R: i32 = 255;
const KEY_B: i32 = 254;
/// Least magenta excess, min(R,B) - G, that counts as key showing through.
/// Below it a pixel is taken as the page's own colour and left alone. Real
/// page colours rarely have red and blue both well above green; a pure blue
/// or a pure red has one of them low and reads as zero here.
const SPILL_MIN: i32 = 12;
/// Coverage below which a pixel over the key is simply the key: alpha zero.
const COVER_MIN: i32 = 10;

/// How far the sample count may run from the wall clock before audio is
/// re-anchored. Two video frames at 30 fps.
const AUDIO_DRIFT_TOLERANCE_NS: u64 = 66_000_000;

/// `lut[alpha * 256 + channel]` is that channel divided by its alpha again.
///
/// Skia gives `on_paint` premultiplied BGRA and GStreamer's BGRA means straight
/// alpha, so without this every half transparent pixel composites too dark. The
/// arithmetic is a divide per colour channel, three per pixel, which is 2.7
/// million divides a frame at 1280x720 and more than the whole rest of the
/// paint path costs. A byte can only take 256 values and so can its alpha, so
/// the entire answer fits in 65536 bytes worked out once at startup.
fn unpremultiply_table() -> Vec<u8> {
    let mut lut = vec![0u8; 256 * 256];
    // Row zero stays zero: nothing was painted there, so there is no colour to
    // recover and dividing by the alpha would be dividing by nothing.
    for a in 1..256usize {
        for c in 0..256usize {
            lut[a * 256 + c] = (c * 255 / a).min(255) as u8;
        }
    }
    lut
}

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
        transparent: bool,
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
        let vfmt = mk("capsfilter")?;
        // BGRA cannot go into Matroska at all, and I420 has nowhere to put the
        // alpha. Of the raw formats `matroskamux` advertises (YUY2, I420, YV12,
        // UYVY, AYUV, GRAY8, GRAY10_LE32, GRAY16_LE, BGR, RGB, RGBA64_LE,
        // BGRA64_LE) AYUV is the only eight bit one that keeps it, so that is
        // what transparent mode converts to. Checked with gst-inspect-1.0.
        vfmt.set_property(
            "caps",
            gst::Caps::builder("video/x-raw")
                .field("format", if transparent { "AYUV" } else { "I420" })
                .build(),
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
            .add_many([&velem, &vconv, &vfmt, &vq, &aelem, &aconv, &aq, &mux, &sink])
            .map_err(|e| e.to_string())?;
        gst::Element::link_many([&velem, &vconv, &vfmt, &vq, &mux]).map_err(|e| e.to_string())?;
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
            unpremultiply: transparent.then(unpremultiply_table),
            dropped: std::sync::atomic::AtomicU64::new(0),
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
        let buf = latest.get_or_insert_with(|| vec![0u8; self.frame_bytes]);
        buf.copy_from_slice(bgra);
        if let Some(lut) = &self.unpremultiply {
            for px in buf.chunks_exact_mut(4) {
                // The key colour the injected script paints where a video the
                // mixer draws itself used to be. Keyed here, where every pixel
                // is being visited anyway, rather than by a chroma key element
                // in the mixer: that one managed four frames a second at 720p
                // on a busy machine, and a page whose frames arrive late is a
                // page the compositor drops as old.
                //
                // Not only the pure key. A caption's dark gradient, a control
                // bar, the soft edge of a letter: anything the page draws over
                // its video with some transparency has been blended with the
                // key by the browser, and comes out tinted. Chromium gave us
                // P = a*C + (1-a)*K for content C at coverage a, and the key
                // is red and blue with no green at all, so for the neutral
                // colours such overlays are made of, min(R,B) - G reads (1-a)
                // straight off the pixel. C and a follow, and the mixer blends
                // the recovered overlay over its own decode of the video, which
                // is how the page looked in the browser. Page content that is
                // itself magenta is taken for spill; that is the trade.
                let (b, g, r) = (px[0] as i32, px[1] as i32, px[2] as i32);
                let spill = b.min(r) - g;
                if spill > SPILL_MIN {
                    let cover = 255 - spill;
                    if cover <= COVER_MIN {
                        px[3] = 0;
                    } else {
                        px[0] = ((b * 255 - spill * KEY_B) / cover).clamp(0, 255) as u8;
                        px[1] = ((g * 255) / cover).clamp(0, 255) as u8;
                        px[2] = ((r * 255 - spill * KEY_R) / cover).clamp(0, 255) as u8;
                        px[3] = ((px[3] as i32 * cover) / 255) as u8;
                    }
                    continue;
                }
                let a = px[3] as usize;
                // Opaque is the common case and the table would return the
                // channel unchanged, so skip the three lookups.
                if a == 255 {
                    continue;
                }
                let row = &lut[a * 256..(a + 1) * 256];
                px[0] = row[px[0] as usize];
                px[1] = row[px[1] as usize];
                px[2] = row[px[2] as usize];
            }
        }
    }

    /// The browser started (or restarted) an audio stream. The next packet
    /// sets a fresh anchor rather than continuing the old sample count.
    pub fn audio_stream_started(&self) {
        *self.audio_clock.lock().unwrap() = None;
    }

    /// Interleaved float PCM straight from the browser, with the presentation
    /// time Chromium gave it (milliseconds since the Unix epoch; 0 if none).
    /// Whether `src` already holds more than `limit` bytes the reader has not
    /// taken, in which case the next push is dropped rather than queued.
    ///
    /// This is a live source and the reader is the mixer at the other end of
    /// a pipe. When it falls behind, even briefly, an appsrc left to itself
    /// queues everything it is given, and at 41 MB/s of raw frames that was 3
    /// GB inside a minute, measured: the container was then killed for memory
    /// or lost its X server first and painted black until it was. Dropping is
    /// what a live feed should do; the mixer holds the last frame it has and
    /// the picture skips instead of the source dying.
    fn behind(src: &gst_app::AppSrc, limit: u64, dropped: &std::sync::atomic::AtomicU64) -> bool {
        if src.current_level_bytes() <= limit {
            return false;
        }
        let n = dropped.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if n == 1 || n % 300 == 0 {
            eprintln!("[browser] the reader is not keeping up; dropped {n} buffers so far");
        }
        true
    }

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
        // A second of audio.
        if Self::behind(&self.audio, 48_000 * 2 * 4, &self.dropped) {
            return;
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
                        // Four frames.
                        if !Self::behind(&self.video, 4 * self.frame_bytes as u64, &self.dropped)
                            && self.video.push_buffer(buf).is_err()
                        {
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
