//! A main input with a backup: both run all the time, so the backup is warm
//! when it is needed and the main is known to be back the moment it is.
//!
//! The switch to the backup happens when the main has sent no frame for
//! `stall_ms` (default 2 s), at the backup's next keyframe. The switch back
//! happens when the main has been steady for `return_ms` (default 5 s), at
//! the main's next keyframe. Each switch sends the incoming side's codec
//! headers first and lays its timeline after the last tag sent, so a reader
//! sees one stream that never goes backwards.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::super::{Input, Sink, StopSignal, TagSink};
use super::spec::InputSpec;
use super::stats::InputStats;
use crate::media_tag::{MediaTag, TagKind};

pub struct Backup {
    main: Box<dyn Input>,
    backup: Box<dyn Input>,
    stall_ms: u64,
    return_ms: u64,
}

impl Backup {
    pub fn new(main: Box<dyn Input>, backup: Box<dyn Input>, spec: &InputSpec) -> Backup {
        let stall_ms = spec.number("stall_ms").unwrap_or(2_000).clamp(250, 60_000);
        let return_ms = spec.number("return_ms").unwrap_or(5_000).clamp(0, 600_000);
        Backup { main, backup, stall_ms, return_ms }
    }
}

#[derive(Default)]
struct Leg {
    /// When its last frame came, and since when it has been steady.
    last: Option<Instant>,
    steady_since: Option<Instant>,
    has_video: bool,
    video_header: Option<MediaTag>,
    audio_header: Option<MediaTag>,
    offset: i64,
    stats: InputStats,
}

struct Switch {
    out: Sink,
    legs: [Leg; 2],
    active: usize,
    want: usize,
    end_ms: i64,
    started: Instant,
}

impl Switch {
    fn tag(&mut self, side: usize, tag: MediaTag) {
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
    fn decide(&mut self, stall: Duration, settle: Duration) {
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

    fn stats(&mut self, side: usize, s: &InputStats) {
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

struct Side(Arc<Mutex<Switch>>, usize);

impl Side {
    fn lock(&self) -> MutexGuard<'_, Switch> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl TagSink for Side {
    fn tag(&mut self, tag: MediaTag) {
        self.lock().tag(self.1, tag);
    }

    fn stats(&mut self, stats: &InputStats) {
        self.lock().stats(self.1, stats);
    }
}

impl Input for Backup {
    fn run(self: Box<Self>, out: Sink, stop: StopSignal) {
        let switch = Switch { out, legs: Default::default(), active: 0, want: 0, end_ms: 0, started: Instant::now() };
        let switch = Arc::new(Mutex::new(switch));
        let (stall, settle) = (Duration::from_millis(self.stall_ms), Duration::from_millis(self.return_ms));
        let Backup { main, backup, .. } = *self;
        let threads: Vec<_> = [main, backup]
            .into_iter()
            .enumerate()
            .map(|(i, input)| {
                let (side, stop) = (Box::new(Side(switch.clone(), i)), stop.clone());
                std::thread::Builder::new().name(format!("direct-in-{}", ["main", "backup"][i])).spawn(move || input.run(side, stop))
            })
            .collect();
        while !stop.wait(Duration::from_millis(250)) {
            switch.lock().unwrap_or_else(|e| e.into_inner()).decide(stall, settle);
        }
        for t in threads.into_iter().flatten() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
#[path = "backup_tests.rs"]
mod tests;
