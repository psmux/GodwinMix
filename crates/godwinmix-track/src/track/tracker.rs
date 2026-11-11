//! The tracker: its state, the public calls, and each track's life from
//! tentative to confirmed to lost to gone.

use std::time::Instant;

use super::assoc::Assoc;
use super::{BoxFilter, Histogram, Track, TrackId, TrackState, TrackerConfig};
use crate::detect::Detection;
use crate::frame::FrameRef;

/// One track and what the tracker keeps beside it.
#[derive(Clone, Debug)]
pub(super) struct Slot {
    pub track: Track,
    pub kf: BoxFilter,
    pub hist: Histogram,
    pub matched: bool,
}

/// A multi object tracker. Feed it every frame's detections with
/// [`update`](Tracker::update), or [`coast`](Tracker::coast) on frames that
/// were not run through a detector.
#[derive(Debug)]
pub struct Tracker {
    pub(super) cfg: TrackerConfig,
    pub(super) slots: Vec<Slot>,
    pub(super) assoc: Assoc,
    pub(super) det_hist: Vec<Histogram>,
    pub(super) det_used: Vec<bool>,
    pub(super) out: Vec<Track>,
    pub(super) next_id: u32,
    pub(super) last_t: Option<Instant>,
    pub(super) frames: u64,
}

impl Default for Tracker {
    fn default() -> Self {
        Self::new(TrackerConfig::default())
    }
}

impl Tracker {
    pub fn new(cfg: TrackerConfig) -> Self {
        Self {
            cfg,
            slots: Vec::with_capacity(32),
            assoc: Assoc::default(),
            det_hist: Vec::with_capacity(32),
            det_used: Vec::with_capacity(32),
            out: Vec::with_capacity(32),
            next_id: 1,
            last_t: None,
            frames: 0,
        }
    }

    pub fn config(&self) -> &TrackerConfig {
        &self.cfg
    }

    /// Matches this frame's detections, taken at `t`, and returns the
    /// confirmed and lost tracks. Without a frame there are no colour
    /// histograms, so a hidden person comes back by position alone.
    pub fn update(&mut self, dets: &[Detection], t: Instant) -> &[Track] {
        self.update_with_frame(dets, None, t)
    }

    /// As [`update`](Tracker::update), with the frame the detections came
    /// from, for the colour histograms.
    pub fn update_with_frame(&mut self, dets: &[Detection], frame: Option<&FrameRef>, t: Instant) -> &[Track] {
        self.advance(t);
        self.frames += 1;
        self.det_hist.clear();
        if let Some(f) = frame.filter(|f| f.validate().is_ok()) {
            self.det_hist.extend(dets.iter().map(|d| Histogram::sample(f, &d.rect)));
        }
        self.det_used.clear();
        self.det_used.resize(dets.len(), false);
        self.slots.iter_mut().for_each(|s| s.matched = false);
        self.match_all(dets, t);
        self.retire(t);
        self.start_new(dets, t);
        self.publish()
    }

    /// Moves every track along its velocity to `t` without new detections,
    /// for frames between detector runs. Nothing is marked lost.
    pub fn coast(&mut self, t: Instant) -> &[Track] {
        self.advance(t);
        self.publish()
    }

    /// The tracks from the last call: confirmed and lost, never tentative.
    pub fn tracks(&self) -> &[Track] {
        &self.out
    }

    pub fn get(&self, id: TrackId) -> Option<&Track> {
        self.out.iter().find(|t| t.id == id)
    }

    /// Forgets every track. Ids keep counting up, so an old id is never
    /// reused for somebody else.
    pub fn reset(&mut self) {
        self.slots.clear();
        self.out.clear();
        self.last_t = None;
        self.frames = 0;
    }

    fn advance(&mut self, t: Instant) {
        let dt = self.last_t.map_or(0.0, |l| t.saturating_duration_since(l).as_secs_f32());
        self.last_t = Some(t);
        if dt <= 0.0 {
            return;
        }
        for s in &mut self.slots {
            if s.track.state == TrackState::Lost {
                s.kf.damp((-dt * 2.0).exp());
            }
            s.kf.predict(dt);
        }
    }

    fn publish(&mut self) -> &[Track] {
        self.out.clear();
        for s in &mut self.slots {
            s.track.rect = s.kf.rect();
            s.track.velocity = s.kf.velocity();
            if s.track.state != TrackState::Tentative {
                self.out.push(s.track);
            }
        }
        &self.out
    }
}
