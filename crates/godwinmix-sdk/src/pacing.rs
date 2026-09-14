//! The frame pool and the pacing loop.
//!
//! A source produces frames on its own monotonic clock, starting near zero. The
//! core retimes them onto programme running time, so a plugin never has to know
//! what time the programme thinks it is. What it must do is keep to the canvas
//! frame rate and never allocate a fresh frame buffer per frame, because at
//! 1080p30 that is 93 MB of allocation a second.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::wire::Canvas;

/// A pool of frame sized buffers. Take one, fill it, drop it, get it back.
///
/// The pool never blocks: if every buffer is out it allocates another. That is
/// the right trade for a media thread, where waiting costs a frame and a
/// megabyte costs nothing.
pub struct FramePool {
    free: Mutex<Vec<Vec<u8>>>,
    frame_bytes: usize,
}

impl FramePool {
    /// A pool of `count` buffers of `frame_bytes` each, allocated now.
    pub fn new(frame_bytes: usize, count: usize) -> Arc<FramePool> {
        let free = (0..count).map(|_| vec![0u8; frame_bytes]).collect();
        Arc::new(FramePool {
            free: Mutex::new(free),
            frame_bytes,
        })
    }

    /// A pool sized for I420 video at these caps. Three buffers is enough for
    /// the usual fill, write, and one in flight.
    pub fn for_i420(canvas: Canvas) -> Arc<FramePool> {
        FramePool::new(canvas.i420_frame_bytes(), 3)
    }

    /// A pool sized for AYUV video at these caps.
    pub fn for_ayuv(canvas: Canvas) -> Arc<FramePool> {
        FramePool::new(canvas.ayuv_frame_bytes(), 3)
    }

    pub fn frame_bytes(&self) -> usize {
        self.frame_bytes
    }

    /// How many buffers are sitting idle. For tests and for `plugin.stats`.
    pub fn free_count(&self) -> usize {
        self.free.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    /// Take a buffer. It comes back on drop.
    pub fn take(self: &Arc<Self>) -> Frame {
        let mut buffer = {
            let mut free = self.free.lock().unwrap_or_else(|e| e.into_inner());
            free.pop()
        }
        .unwrap_or_else(|| vec![0u8; self.frame_bytes]);
        buffer.resize(self.frame_bytes, 0);
        Frame {
            buffer: Some(buffer),
            pool: Arc::clone(self),
        }
    }
}

/// A frame buffer on loan from a [`FramePool`].
pub struct Frame {
    buffer: Option<Vec<u8>>,
    pool: Arc<FramePool>,
}

impl Frame {
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_deref().unwrap_or(&[])
    }

    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        self.buffer.as_deref_mut().unwrap_or(&mut [])
    }

    pub fn len(&self) -> usize {
        self.as_slice().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl std::ops::Deref for Frame {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        self.as_slice()
    }
}

impl std::ops::DerefMut for Frame {
    fn deref_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

impl Drop for Frame {
    fn drop(&mut self) {
        if let Some(buffer) = self.buffer.take() {
            let mut free = self.pool.free.lock().unwrap_or_else(|e| e.into_inner());
            // Keep the pool from growing without bound if a plugin leaks
            // frames into a queue and then drops them all at once.
            if free.len() < 16 {
                free.push(buffer);
            }
        }
    }
}

/// Paces a producer to the canvas frame rate and hands out monotonic PTS.
///
/// PTS starts near zero on the plugin's own clock, which is what the media
/// contract asks for. The deadline is computed from the frame count, not by
/// adding a sleep each time, so a slow frame does not push every later frame
/// back.
pub struct Pacer {
    start: Instant,
    frame_duration_ns: u64,
    frame: u64,
    /// Frames whose deadline had already passed when they were asked for.
    late: u64,
}

impl Pacer {
    pub fn new(canvas: Canvas) -> Pacer {
        Pacer {
            start: Instant::now(),
            frame_duration_ns: canvas.frame_duration_ns().max(1),
            frame: 0,
            late: 0,
        }
    }

    /// Restart the clock. Call it on `start`, so PTS begins near zero.
    pub fn restart(&mut self) {
        self.start = Instant::now();
        self.frame = 0;
        self.late = 0;
    }

    /// The PTS of the frame just produced, in nanoseconds.
    pub fn pts_ns(&self) -> u64 {
        self.frame.saturating_mul(self.frame_duration_ns)
    }

    /// How many frames arrived after their deadline. A source reporting this in
    /// `health` tells the operator it cannot keep up before the picture stalls.
    pub fn late_frames(&self) -> u64 {
        self.late
    }

    pub fn frames(&self) -> u64 {
        self.frame
    }

    /// Sleep until the next frame is due, then return its PTS.
    ///
    /// The first call returns 0 without sleeping, so the first frame leaves at
    /// once and the core prerolls.
    ///
    /// Named `next` because that is what it is, not because it is an iterator:
    /// a producer that has to stop cannot do it from inside a `for` loop.
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> u64 {
        let pts = self.pts_ns();
        let deadline = Duration::from_nanos(pts);
        let now = self.start.elapsed();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else if self.frame > 0 && now.saturating_sub(deadline) > Duration::from_nanos(self.frame_duration_ns) {
            self.late += 1;
        }
        self.frame += 1;
        pts
    }

    /// The PTS a pure computation would use for frame `n`. For tests and for a
    /// plugin that produces frames without sleeping, such as an offline replay.
    pub fn pts_of(&self, frame: u64) -> u64 {
        frame.saturating_mul(self.frame_duration_ns)
    }
}

/// Paces audio buffers of 10 ms, which is what the media contract asks for.
///
/// F32LE, 48 kHz, 2 channels: 480 samples per buffer, 3,840 bytes.
pub struct AudioPacer {
    rate: u32,
    channels: u32,
    samples_per_buffer: u32,
    buffers: u64,
}

impl Default for AudioPacer {
    fn default() -> Self {
        AudioPacer::new()
    }
}

impl AudioPacer {
    pub fn new() -> AudioPacer {
        AudioPacer {
            rate: 48_000,
            channels: 2,
            samples_per_buffer: 480,
            buffers: 0,
        }
    }

    /// Bytes in one 10 ms buffer.
    pub fn buffer_bytes(&self) -> usize {
        (self.samples_per_buffer * self.channels) as usize * std::mem::size_of::<f32>()
    }

    pub fn samples_per_buffer(&self) -> u32 {
        self.samples_per_buffer
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn channels(&self) -> u32 {
        self.channels
    }

    /// The PTS of the next buffer, then step forward.
    pub fn next_pts_ns(&mut self) -> u64 {
        let pts = self.buffers * 10_000_000;
        self.buffers += 1;
        pts
    }

    pub fn restart(&mut self) {
        self.buffers = 0;
    }

    /// How many audio buffers belong before this video PTS. A source that draws
    /// video and fills audio in the same loop uses this to stay in step.
    pub fn buffers_due_by(&self, pts_ns: u64) -> u64 {
        pts_ns / 10_000_000
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pool_hands_a_buffer_back_on_drop() {
        let pool = FramePool::new(1024, 2);
        assert_eq!(pool.free_count(), 2);
        {
            let mut frame = pool.take();
            assert_eq!(frame.len(), 1024);
            frame[0] = 7;
            assert_eq!(pool.free_count(), 1);
            let _second = pool.take();
            assert_eq!(pool.free_count(), 0);
            // An empty pool allocates rather than blocking a media thread.
            let _third = pool.take();
            assert_eq!(pool.free_count(), 0);
        }
        assert_eq!(pool.free_count(), 3);
    }

    #[test]
    fn a_pool_sized_for_the_canvas_matches_the_contract() {
        let canvas = Canvas::new(1280, 720, 30);
        let pool = FramePool::for_i420(canvas);
        assert_eq!(pool.frame_bytes(), 1280 * 720 * 3 / 2);
        let ayuv = FramePool::for_ayuv(canvas);
        assert_eq!(ayuv.frame_bytes(), 1280 * 720 * 4);
    }

    #[test]
    fn the_pool_does_not_grow_without_bound() {
        let pool = FramePool::new(16, 0);
        let frames: Vec<Frame> = (0..64).map(|_| pool.take()).collect();
        drop(frames);
        assert!(pool.free_count() <= 16);
    }

    #[test]
    fn pts_is_monotonic_and_starts_at_zero() {
        let mut pacer = Pacer::new(Canvas::new(64, 64, 1000));
        let mut last = None;
        for _ in 0..5 {
            let pts = pacer.next();
            if let Some(previous) = last {
                assert!(pts > previous, "{pts} after {previous}");
            } else {
                assert_eq!(pts, 0, "the first frame leaves at once");
            }
            last = Some(pts);
        }
        assert_eq!(pacer.frames(), 5);
    }

    #[test]
    fn the_pacer_sleeps_to_the_frame_rate() {
        // 200 fps: five frames is 20 ms of wall clock, plus the first at once.
        let mut pacer = Pacer::new(Canvas::new(64, 64, 200));
        let start = Instant::now();
        for _ in 0..5 {
            pacer.next();
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(18),
            "five frames at 200 fps took {elapsed:?}, which is too fast"
        );
        assert!(
            elapsed < Duration::from_millis(200),
            "five frames at 200 fps took {elapsed:?}, which is far too slow"
        );
    }

    #[test]
    fn a_slow_frame_does_not_push_the_rest_back() {
        let mut pacer = Pacer::new(Canvas::new(64, 64, 30));
        pacer.next();
        std::thread::sleep(Duration::from_millis(70));
        // The deadline for frame 1 and frame 2 has passed, so neither sleeps
        // and the PTS keeps the nominal spacing.
        assert_eq!(pacer.next(), 33_333_333);
        assert_eq!(pacer.next(), 66_666_666);
        assert!(pacer.late_frames() >= 1);
    }

    #[test]
    fn restart_takes_pts_back_to_zero() {
        let mut pacer = Pacer::new(Canvas::new(64, 64, 1000));
        pacer.next();
        pacer.next();
        pacer.restart();
        assert_eq!(pacer.next(), 0);
    }

    #[test]
    fn audio_buffers_are_ten_milliseconds_of_stereo_f32() {
        let mut audio = AudioPacer::new();
        assert_eq!(audio.buffer_bytes(), 480 * 2 * 4);
        assert_eq!(audio.next_pts_ns(), 0);
        assert_eq!(audio.next_pts_ns(), 10_000_000);
        assert_eq!(audio.buffers_due_by(1_000_000_000), 100);
    }
}
