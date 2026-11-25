//! A stretch of one ring, taken out to play or to save.
//!
//! A clip starts at the keyframe at or before its in point, because nothing
//! after a keyframe can be decoded without it, and remembers the in point so
//! a player can decode the frames before it without showing them. The frames
//! are the ring's own buffers, shared by reference: a thirty second clip of a
//! 6 Mbit/s angle costs tens of kilobytes of bookkeeping, not 22 MB.

use super::ring::{Frame, Ring};
use gstreamer as gst;

/// A picture frame, with the caps to set before it when they change there.
#[derive(Clone)]
pub struct ClipFrame {
    pub caps: Option<gst::Caps>,
    pub frame: Frame,
}

#[derive(Clone, Default)]
pub struct Clip {
    /// The in point asked for, in clock nanoseconds.
    pub from: u64,
    /// The out point, or where the ring ended when the clip was taken.
    pub to: u64,
    pub video: Vec<ClipFrame>,
    pub audio: Vec<Frame>,
    pub audio_caps: Option<gst::Caps>,
}

impl Clip {
    /// Nanoseconds from the in point to the out point.
    pub fn length(&self) -> u64 {
        self.to.saturating_sub(self.from)
    }

    pub fn is_empty(&self) -> bool {
        self.video.is_empty()
    }

    /// The caps of the first picture, which every player starts from.
    pub fn video_caps(&self) -> Option<&gst::Caps> {
        self.video.first().and_then(|f| f.caps.as_ref())
    }

    /// The index of the last keyframe at or before `at`, for a seek.
    pub fn key_before(&self, at: u64) -> usize {
        let mut best = 0;
        for (i, f) in self.video.iter().enumerate() {
            if f.frame.at > at {
                break;
            }
            if f.frame.key {
                best = i;
            }
        }
        best
    }

    /// The caps in force at frame `i`: the nearest set at or before it.
    pub fn caps_at(&self, i: usize) -> Option<&gst::Caps> {
        self.video[..=i.min(self.video.len().saturating_sub(1))].iter().rev().find_map(|f| f.caps.as_ref())
    }

    /// The first sound frame at or after `at`.
    pub fn audio_from(&self, at: u64) -> usize {
        self.audio.partition_point(|f| f.at < at)
    }
}

impl Ring {
    /// Everything between `from` and `to`, from the keyframe before `from`.
    /// Empty when the ring holds nothing of that stretch.
    pub fn clip(&self, from: u64, to: u64) -> Clip {
        let mut clip = Clip { from, to, audio_caps: self.audio_caps.clone(), ..Clip::default() };
        let start = self.gops.iter().rposition(|g| g.start() <= from).unwrap_or(0);
        for gop in self.gops.iter().skip(start) {
            if gop.start() >= to {
                break;
            }
            let mut first = true;
            for f in gop.frames.iter().filter(|f| f.at < to) {
                let caps = first.then(|| gop.caps.clone());
                first = false;
                clip.video.push(ClipFrame { caps, frame: f.clone() });
            }
        }
        let Some(head) = clip.video.first().map(|f| f.frame.at) else { return clip };
        // A ring asked for a stretch it has already dropped starts later.
        clip.from = clip.from.max(head);
        clip.audio = self.audio.iter().filter(|f| f.at >= head && f.at < to).cloned().collect();
        clip
    }

    /// The whole ring as one clip.
    pub fn everything(&self) -> Clip {
        match self.span() {
            Some((a, b)) => self.clip(a, b),
            None => Clip::default(),
        }
    }
}
