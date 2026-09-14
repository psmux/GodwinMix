//! The loop that turns a `draw` function into a source.
//!
//! A plugin author should not have to get pacing, pooling and PTS right to put
//! a picture on the screen. `VideoLoop::spawn` takes a closure that fills an
//! I420 (or AYUV) buffer and does the rest: a pool so no frame is allocated
//! twice, a pacer so the frames leave at canvas rate, and PTS on the plugin's
//! own monotonic clock starting at zero.
//!
//! The loop owns its thread. `stop` asks it to finish the frame it is on and
//! join, and dropping it does the same, so a plugin cannot leave a thread
//! writing into a closed pipe.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use crate::media::MediaWriter;
use crate::pacing::{AudioPacer, FramePool, Pacer};
use crate::plugin::Reporter;
use crate::wire::{Canvas, LogLevel};

/// A running producer thread.
pub struct VideoLoop {
    stop: Arc<AtomicBool>,
    frames: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl VideoLoop {
    /// Start producing. `draw` is called once per frame with the buffer to fill
    /// and the PTS of that frame in nanoseconds.
    ///
    /// `draw` runs on the media thread. Keep it to drawing: a network call in
    /// there is a dropped frame.
    pub fn spawn<F>(
        canvas: Canvas,
        writer: Box<dyn MediaWriter>,
        reporter: Option<Reporter>,
        draw: F,
    ) -> VideoLoop
    where
        F: FnMut(&mut [u8], u64) + Send + 'static,
    {
        VideoLoop::spawn_with_audio(canvas, writer, reporter, draw, None::<fn(&mut [u8], u64)>)
    }

    /// Start producing picture and sound. `fill_audio` is called for each 10 ms
    /// buffer due before the next video frame.
    pub fn spawn_with_audio<F, A>(
        canvas: Canvas,
        mut writer: Box<dyn MediaWriter>,
        reporter: Option<Reporter>,
        mut draw: F,
        fill_audio: Option<A>,
    ) -> VideoLoop
    where
        F: FnMut(&mut [u8], u64) + Send + 'static,
        A: FnMut(&mut [u8], u64) + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let frames = Arc::new(AtomicU64::new(0));
        let thread_stop = Arc::clone(&stop);
        let thread_frames = Arc::clone(&frames);
        let pool = FramePool::for_i420(canvas);
        let handle = std::thread::Builder::new()
            .name("gmx-media".into())
            .spawn(move || {
                let mut pacer = Pacer::new(canvas);
                let mut audio = AudioPacer::new();
                let mut audio_buffer = vec![0u8; audio.buffer_bytes()];
                let mut fill_audio = fill_audio;
                let mut audio_written: u64 = 0;
                while !thread_stop.load(Ordering::Relaxed) {
                    let pts = pacer.next();
                    let mut frame = pool.take();
                    draw(&mut frame, pts);
                    if let Err(e) = writer.write_video(pts, &frame, true) {
                        report(
                            &reporter,
                            LogLevel::Error,
                            format!("the media pipe closed: {e}"),
                        );
                        break;
                    }
                    thread_frames.fetch_add(1, Ordering::Relaxed);

                    if let Some(fill) = fill_audio.as_mut() {
                        // Keep the sound level with the picture without letting
                        // a slow frame push out a burst of buffers.
                        let due = audio.buffers_due_by(pts + canvas.frame_duration_ns());
                        while audio_written < due {
                            let apts = audio.next_pts_ns();
                            fill(&mut audio_buffer, apts);
                            if let Err(e) = writer.write_audio(apts, &audio_buffer) {
                                report(
                                    &reporter,
                                    LogLevel::Error,
                                    format!("the audio pipe closed: {e}"),
                                );
                                return;
                            }
                            audio_written += 1;
                        }
                    }
                }
                let _ = writer.finish();
                report(
                    &reporter,
                    LogLevel::Debug,
                    format!(
                        "media loop finished after {} frames, {} late",
                        thread_frames.load(Ordering::Relaxed),
                        pacer.late_frames()
                    ),
                );
            })
            .expect("could not start the media thread");
        VideoLoop {
            stop,
            frames,
            handle: Some(handle),
        }
    }

    /// How many frames have gone out. `plugin.stats` and the harness both want
    /// this number.
    pub fn frames(&self) -> u64 {
        self.frames.load(Ordering::Relaxed)
    }

    pub fn is_running(&self) -> bool {
        self.handle.as_ref().is_some_and(|h| !h.is_finished())
    }

    /// Ask the loop to finish the frame it is on, then wait for it.
    pub fn stop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for VideoLoop {
    fn drop(&mut self) {
        self.stop();
    }
}

fn report(reporter: &Option<Reporter>, level: LogLevel, message: String) {
    if let Some(r) = reporter {
        r.log(level, message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::{ContainerWriter, Streams, VideoFormat};
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Sink(Arc<Mutex<Vec<u8>>>);

    impl std::io::Write for Sink {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn the_loop_paces_fills_and_stops() {
        let canvas = Canvas::new(16, 16, 200);
        let sink = Sink::default();
        let writer =
            ContainerWriter::new(sink.clone(), canvas, Streams::video_only(VideoFormat::I420))
                .unwrap();
        let seen = Arc::new(Mutex::new(Vec::<u64>::new()));
        let recorder = Arc::clone(&seen);
        let mut looper = VideoLoop::spawn(canvas, Box::new(writer), None, move |frame, pts| {
            frame.fill(0x80);
            recorder.lock().unwrap().push(pts);
        });
        std::thread::sleep(std::time::Duration::from_millis(60));
        looper.stop();
        let pts = seen.lock().unwrap().clone();
        assert!(
            pts.len() >= 5,
            "only {} frames in 60 ms at 200 fps",
            pts.len()
        );
        assert_eq!(pts[0], 0);
        for pair in pts.windows(2) {
            assert!(pair[1] > pair[0], "PTS must climb: {pair:?}");
        }
        assert_eq!(looper.frames() as usize, pts.len());
        assert!(!looper.is_running());
        assert!(!sink.0.lock().unwrap().is_empty());
    }

    #[test]
    fn dropping_the_loop_stops_the_thread() {
        let canvas = Canvas::new(8, 8, 500);
        let writer =
            ContainerWriter::new(Vec::new(), canvas, Streams::video_only(VideoFormat::I420))
                .unwrap();
        let looper = VideoLoop::spawn(canvas, Box::new(writer), None, |frame, _| frame.fill(1));
        std::thread::sleep(std::time::Duration::from_millis(20));
        drop(looper);
        // Nothing to assert but the absence of a hang; a leaked thread writing
        // into a closed pipe is what this guards against.
    }

    #[test]
    fn audio_buffers_keep_up_with_the_picture() {
        let canvas = Canvas::new(8, 8, 100);
        let sink = Sink::default();
        let writer = ContainerWriter::new(
            sink.clone(),
            canvas,
            Streams::video_and_audio(VideoFormat::I420),
        )
        .unwrap();
        let audio_count = Arc::new(AtomicU64::new(0));
        let counter = Arc::clone(&audio_count);
        let mut looper = VideoLoop::spawn_with_audio(
            canvas,
            Box::new(writer),
            None,
            |frame, _| frame.fill(0),
            Some(move |buffer: &mut [u8], _pts: u64| {
                buffer.fill(0);
                counter.fetch_add(1, Ordering::Relaxed);
            }),
        );
        std::thread::sleep(std::time::Duration::from_millis(120));
        looper.stop();
        // 100 fps is one video frame per 10 ms, so audio and video stay level.
        let audio = audio_count.load(Ordering::Relaxed);
        let video = looper.frames();
        assert!(audio >= video, "{audio} audio buffers for {video} frames");
    }
}
