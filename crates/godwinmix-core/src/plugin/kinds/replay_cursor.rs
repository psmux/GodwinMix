//! Where a replay feed is in its clip, and the times it gives each frame.
//!
//! A frame's time is its distance from the in point divided by the speed.
//! Frames between the keyframe a clip starts from and the moment it is to
//! show go out flagged `DECODE_ONLY`: the decoder decodes them and shows none,
//! so a play or a seek lands on the exact frame asked for.

use crate::replay::clip::Clip;
use crate::replay::ring::Frame;
use gstreamer as gst;

pub(super) struct State {
    pub(super) clip: Clip,
    pub(super) speed: f64,
    /// Where showing starts, in clock nanoseconds of the clip.
    pub(super) show_from: u64,
    pub(super) video: usize,
    pub(super) audio: usize,
    /// Set by a load or a change of angle, cleared by the seek that follows:
    /// nothing is pushed in between, so no frame of the new clip goes out at
    /// the old one's place.
    pub(super) held: bool,
    /// When playback last started from a position, on the pipeline's clock:
    /// what is on screen is that position plus the time since.
    pub(super) since: Option<(gst::Clock, u64, u64)>,
}

impl State {
    pub(super) fn new(clip: Clip, speed: f64) -> Self {
        let mut s = Self { clip, speed, show_from: 0, video: 0, audio: 0, held: false, since: None };
        s.seek(0);
        s
    }

    pub(super) fn out(&self, at: u64) -> u64 {
        (at.saturating_sub(self.clip.from) as f64 / self.speed) as u64
    }

    pub(super) fn seek(&mut self, position: u64) {
        self.since = None;
        self.show_from = self.clip.from + (position as f64 * self.speed) as u64;
        self.video = self.clip.key_before(self.show_from);
        self.audio = self.clip.audio_from(self.show_from);
        self.held = false;
    }

    pub(super) fn stamp(&self, f: &Frame, hidden: bool) -> gst::Buffer {
        let mut b = f.buffer.copy();
        let buf = b.make_mut();
        buf.set_pts(gst::ClockTime::from_nseconds(self.out(f.at)));
        buf.set_dts(f.dts.map(|d| gst::ClockTime::from_nseconds(self.out(d))));
        buf.set_duration(gst::ClockTime::from_nseconds((f.duration as f64 / self.speed) as u64));
        if hidden {
            buf.set_flags(gst::BufferFlags::DECODE_ONLY);
        }
        b
    }
}
