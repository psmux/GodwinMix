//! The switch between a main input and its backup: which one is handed on,
//! and the one timeline both are laid on.

use std::time::{Duration, Instant};

use super::super::super::Sink;
use super::super::stats::InputStats;
use crate::media_tag::{MediaTag, TagKind};

#[derive(Default)]
pub struct Leg {
    /// When its last frame came, and since when it has been steady.
    pub last: Option<Instant>,
    pub steady_since: Option<Instant>,
    pub has_video: bool,
    pub video_header: Option<MediaTag>,
    pub audio_header: Option<MediaTag>,
    pub offset: i64,
    pub stats: InputStats,
}

pub struct Switch {
    pub out: Sink,
    pub legs: [Leg; 2],
    pub active: usize,
    pub want: usize,
    pub end_ms: i64,
    pub started: Instant,
}

impl Switch {
    pub fn tag(&mut self, side: usize, tag: MediaTag) {
        let leg = &mut self.legs[side];
        match tag.kind {
            TagKind::Video if tag.sequence_header => leg.video_header = Some(tag.clone()),
            TagKind::Audio if tag.sequence_header => leg.audio_header = Some(tag.clone()),
            TagKind::Video | TagKind::Audio => {
                let now = Instant::now();
                let gap = leg.last.map_or(u64::MAX, |l| now.duration_since(l).as_millis() as u64);
                if gap > 1_000 {
                    leg.steady_since = Some(now);
                }
                leg.last = Some(now);
                leg.has_video |= tag.kind == TagKind::Video;
            }
            TagKind::Script => {}
        }
        if side != self.active {
            let can_cut = !self.legs[side].has_video || (tag.kind == TagKind::Video && tag.keyframe && !tag.sequence_header);
            if self.want != side || !can_cut {
                return;
            }
            self.cut_to(side, tag.timestamp_ms);
        }
        self.send(side, tag);
    }

    /// Make `side` the one handed on, its headers first.
    fn cut_to(&mut self, side: usize, at_ms: u32) {
        self.active = side;
        self.legs[side].offset = self.end_ms + 40 - i64::from(at_ms);
        for header in [self.legs[side].video_header.clone(), self.legs[side].audio_header.clone()].into_iter().flatten() {
            self.send(side, MediaTag { timestamp_ms: at_ms, ..header });
        }
    }

    fn send(&mut self, side: usize, mut tag: MediaTag) {
        let ms = (i64::from(tag.timestamp_ms) + self.legs[side].offset).max(0);
        self.end_ms = self.end_ms.max(ms);
        tag.timestamp_ms = ms as u32;
        self.out.tag(tag);
    }

    /// Choose a side, from how long each has been quiet or steady.
    pub fn decide(&mut self, stall: Duration, settle: Duration) {
        let quiet = |l: &Leg| l.last.map_or(self.started.elapsed(), |t| t.elapsed());
        let main_down = quiet(&self.legs[0]) > stall;
        let backup_up = quiet(&self.legs[1]) < stall;
        let main_steady = !main_down && self.legs[0].steady_since.is_some_and(|s| s.elapsed() >= settle);
        self.want = match self.active {
            0 if main_down && backup_up => 1,
            1 if main_steady => 0,
            a => a,
        };
    }

    pub fn stats(&mut self, side: usize, s: &InputStats) {
        self.legs[side].stats = s.clone();
        if side == self.active {
            let mut s = s.clone();
            if side == 1 {
                let why = self.legs[0].stats.error.clone().unwrap_or_else(|| "the main input stalled".into());
                s.error = Some(format!("on the backup input: {why}"));
            }
            self.out.stats(&s);
        }
    }
}
