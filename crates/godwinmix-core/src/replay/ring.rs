//! One angle's memory: whole groups of pictures, oldest first, and the sound
//! beside them.
//!
//! The ring only ever drops a whole GOP, from the front, and only while what
//! is left still covers the depth asked for. So it holds at least `depth` of
//! picture once it has been running that long, and never more than `depth`
//! plus the one GOP it is about to drop. Every frame is the encoder's own
//! buffer, held by reference: nothing is copied going in or coming out.
//!
//! Times are nanoseconds of the clock the pipelines share, so two angles
//! recorded from two pipelines line up, and a mark made on the programme's
//! clock finds the same moment in each.

pub use super::frame::Frame;
use gstreamer as gst;
use std::collections::VecDeque;

/// A keyframe and everything up to the next one, with the caps it came with.
pub struct Gop {
    pub caps: gst::Caps,
    pub frames: Vec<Frame>,
    bytes: u64,
}

impl Gop {
    pub fn start(&self) -> u64 {
        self.frames.first().map(|f| f.at).unwrap_or(0)
    }

    fn end(&self) -> u64 {
        self.frames.iter().map(Frame::end).max().unwrap_or(0)
    }
}

pub struct Ring {
    depth: u64,
    pub(super) gops: VecDeque<Gop>,
    pub(super) audio: VecDeque<Frame>,
    pub(super) audio_caps: Option<gst::Caps>,
    bytes: u64,
    /// Pictures that came before the first keyframe and so had nothing to
    /// belong to.
    pub orphans: u64,
}

impl Ring {
    pub fn new(depth_secs: u32) -> Self {
        Self {
            depth: u64::from(depth_secs.max(1)) * 1_000_000_000,
            gops: VecDeque::new(),
            audio: VecDeque::new(),
            audio_caps: None,
            bytes: 0,
            orphans: 0,
        }
    }

    pub fn set_depth(&mut self, depth_secs: u32) {
        self.depth = u64::from(depth_secs.max(1)) * 1_000_000_000;
        self.evict();
    }

    pub fn push_video(&mut self, frame: Frame, caps: &gst::Caps) {
        if frame.key {
            self.gops.push_back(Gop { caps: caps.clone(), frames: Vec::new(), bytes: 0 });
        } else if self.gops.back().is_none_or(|g| &g.caps != caps) {
            // A delta frame with nothing to decode it from.
            self.orphans += 1;
            return;
        }
        let gop = self.gops.back_mut().expect("a GOP was just made or is open");
        self.bytes += frame.bytes();
        gop.bytes += frame.bytes();
        gop.frames.push(frame);
        self.evict();
    }

    pub fn push_audio(&mut self, frame: Frame, caps: &gst::Caps) {
        if self.audio_caps.as_ref() != Some(caps) {
            self.audio_caps = Some(caps.clone());
        }
        self.bytes += frame.bytes();
        self.audio.push_back(frame);
        self.evict();
    }

    /// Drop whole GOPs from the front while the rest still covers the depth,
    /// and the sound from before the oldest picture left.
    fn evict(&mut self) {
        let newest = self.gops.back().map(Gop::end).unwrap_or(0);
        while self.gops.len() > 1 && newest.saturating_sub(self.gops[1].start()) >= self.depth {
            let gone = self.gops.pop_front().expect("more than one GOP");
            self.bytes -= gone.bytes;
        }
        let oldest = match self.gops.front() {
            Some(g) => g.start(),
            // Sound with no picture yet keeps the depth by itself.
            None => self.audio.back().map(|f| f.end().saturating_sub(self.depth)).unwrap_or(0),
        };
        while self.audio.front().is_some_and(|f| f.at < oldest) {
            let gone = self.audio.pop_front().expect("checked");
            self.bytes -= gone.bytes();
        }
    }

    /// Bytes held, picture and sound.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn gop_count(&self) -> usize {
        self.gops.len()
    }

    /// Nanoseconds of picture held, oldest keyframe to newest frame's end.
    pub fn held(&self) -> u64 {
        match (self.gops.front(), self.gops.back()) {
            (Some(a), Some(b)) => b.end().saturating_sub(a.start()),
            _ => 0,
        }
    }

    /// The oldest and newest moment a picture is held for.
    pub fn span(&self) -> Option<(u64, u64)> {
        Some((self.gops.front()?.start(), self.gops.back()?.end()))
    }

    pub fn depth(&self) -> u64 {
        self.depth
    }

    /// The biggest GOP held, for checking the memory bound.
    pub fn largest_gop_bytes(&self) -> u64 {
        self.gops.iter().map(|g| g.bytes).max().unwrap_or(0)
    }
}
