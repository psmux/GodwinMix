//! Playing a replay clip into the two appsrcs of a replay source.
//!
//! Every frame is the ring's own buffer with new times: its distance from the
//! in point, stretched by the speed. At half speed a frame lasts two canvas
//! frames and `videorate` in the normaliser shows it twice, so the programme
//! sees a picture at its own rate with timestamps that keep counting the way a
//! live source's do. The sound plays at real time only; slower, it is muted
//! (`muted`), because stretched AAC is silence with clicks in it.
//!
//! Frames between the keyframe a clip starts from and the moment it is to
//! show go out flagged `DECODE_ONLY`: the decoder decodes them and shows
//! none, so a play or a seek lands on the exact frame asked for. A seek is
//! the appsrcs' `seek-data`, which moves both cursors.
//!
//! The callbacks run on the appsrcs' streaming threads and on whatever thread
//! seeks. Each takes the one lock for a few index moves and a buffer copy that
//! shares its memory.

use crate::replay::clip::Clip;
use crate::replay::ring::Frame;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use parking_lot::Mutex;
use std::sync::Arc;

/// Frames handed over per `need-data`.
const BATCH: usize = 4;

pub struct Feed {
    state: Mutex<State>,
}

struct State {
    clip: Clip,
    speed: f64,
    /// Where showing starts, in clock nanoseconds of the clip.
    show_from: u64,
    video: usize,
    audio: usize,
    /// Set by a load or a change of angle, cleared by the seek that follows:
    /// nothing is pushed in between, so no frame of the new clip goes out at
    /// the old one's place.
    held: bool,
    /// When playback last started from a position, on the pipeline's clock:
    /// what is on screen is that position plus the time since.
    since: Option<(gst::Clock, u64, u64)>,
}

impl State {
    fn out(&self, at: u64) -> u64 {
        (at.saturating_sub(self.clip.from) as f64 / self.speed) as u64
    }

    fn seek(&mut self, position: u64) {
        self.since = None;
        self.show_from = self.clip.from + (position as f64 * self.speed) as u64;
        self.video = self.clip.key_before(self.show_from);
        self.audio = self.clip.audio_from(self.show_from);
        self.held = false;
    }

    fn stamp(&self, f: &Frame, hidden: bool) -> gst::Buffer {
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

impl Feed {
    pub fn new(clip: Clip, speed: f64) -> Arc<Self> {
        let mut state = State { clip, speed, show_from: 0, video: 0, audio: 0, held: false, since: None };
        state.seek(0);
        Arc::new(Self { state: Mutex::new(state) })
    }

    /// How long the clip plays at its speed, in nanoseconds.
    pub fn duration(&self) -> u64 {
        let s = self.state.lock();
        (s.clip.length() as f64 / s.speed) as u64
    }

    pub fn speed(&self) -> f64 {
        self.state.lock().speed
    }

    pub fn muted(&self) -> bool {
        (self.speed() - 1.0).abs() > 1e-6
    }

    /// Where playback is on screen, in nanoseconds: where it last started
    /// from plus the time since, up to its end. The pipeline's own position
    /// is where the decoder has got to, which runs ahead by what the queues
    /// between it and the programme hold.
    pub fn position(&self) -> u64 {
        let s = self.state.lock();
        let length = (s.clip.length() as f64 / s.speed) as u64;
        let Some((clock, at, from)) = &s.since else { return 0 };
        (from + clock.time().nseconds().saturating_sub(*at)).min(length)
    }

    /// Where `position` of the playing clip lands once the speed is `speed`.
    pub fn position_at(&self, position: u64, speed: f64) -> u64 {
        let s = self.state.lock();
        (position as f64 * s.speed / speed) as u64
    }

    /// Swap the clip or the speed, and push nothing until the next seek.
    pub fn replace(&self, clip: Clip, speed: f64) {
        let mut s = self.state.lock();
        s.clip = clip;
        s.speed = speed;
        s.held = true;
    }

    pub fn first_caps(&self) -> Option<gst::Caps> {
        self.state.lock().clip.video_caps().cloned()
    }

    pub fn audio_caps(&self) -> Option<gst::Caps> {
        let s = self.state.lock();
        s.clip.audio_caps.clone().filter(|_| !s.clip.audio.is_empty())
    }

    pub fn wire(self: &Arc<Self>, video: &gst_app::AppSrc, audio: Option<&gst_app::AppSrc>) {
        let (feed, seeker) = (self.clone(), self.clone());
        video.set_callbacks(
            gst_app::AppSrcCallbacks::builder()
                .need_data(move |src, _| feed.push_video(src))
                .seek_data(move |_, position| {
                    seeker.state.lock().seek(position);
                    true
                })
                .build(),
        );
        if let Some(audio) = audio {
            let feed = self.clone();
            audio.set_callbacks(
                gst_app::AppSrcCallbacks::builder()
                    .need_data(move |src, _| feed.push_audio(src))
                    // The picture's seek has moved both cursors.
                    .seek_data(|_, _| true)
                    .build(),
            );
        }
    }

    fn push_video(&self, src: &gst_app::AppSrc) {
        let mut s = self.state.lock();
        if s.held {
            return;
        }
        if s.since.is_none() {
            let start = s.out(s.show_from);
            s.since = src.clock().map(|c| {
                let now = c.time().nseconds();
                (c, now, start)
            });
        }
        for _ in 0..BATCH {
            let Some(cf) = s.clip.video.get(s.video).cloned() else {
                let _ = src.end_of_stream();
                return;
            };
            if let Some(caps) = s.clip.caps_at(s.video).filter(|c| src.caps().as_ref() != Some(*c)) {
                src.set_caps(Some(caps));
            }
            let buffer = s.stamp(&cf.frame, cf.frame.at < s.show_from);
            s.video += 1;
            if src.push_buffer(buffer).is_err() {
                return;
            }
        }
    }

    fn push_audio(&self, src: &gst_app::AppSrc) {
        let mut s = self.state.lock();
        if s.held {
            return;
        }
        for _ in 0..BATCH * 4 {
            let Some(f) = s.clip.audio.get(s.audio).cloned() else {
                let _ = src.end_of_stream();
                return;
            };
            let buffer = s.stamp(&f, false);
            s.audio += 1;
            if src.push_buffer(buffer).is_err() {
                return;
            }
        }
    }
}
