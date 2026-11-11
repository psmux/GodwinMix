//! The matching passes, in ByteTrack's order, and bringing a lost person
//! back by colour when they reappear somewhere the filter did not expect.

use std::time::Instant;

use super::{TrackState, Tracker};
use crate::detect::Detection;

impl Tracker {
    /// Confident detections against confirmed and lost tracks, then weak
    /// detections against the confirmed tracks still unmatched, then what is
    /// left of the confident ones against tentative tracks.
    pub(super) fn match_all(&mut self, dets: &[Detection], t: Instant) {
        let (hi, lo) = (self.cfg.high_score, self.cfg.low_score);
        let strong = |s: TrackState| s != TrackState::Tentative;
        self.pass(dets, t, strong, |d| d.score >= hi, self.cfg.match_iou, true);
        let weak = |d: &Detection| d.score >= lo && d.score < hi;
        self.pass(dets, t, |s| s == TrackState::Confirmed, weak, self.cfg.low_match_iou, false);
        self.pass(dets, t, |s| s == TrackState::Tentative, |d| d.score >= hi, self.cfg.match_iou, true);
    }

    fn pass(
        &mut self,
        dets: &[Detection],
        t: Instant,
        rows: impl Fn(TrackState) -> bool,
        cols: impl Fn(&Detection) -> bool,
        min_iou: f32,
        colour: bool,
    ) {
        let a = &mut self.assoc;
        a.rows.clear();
        a.cols.clear();
        a.rows.extend((0..self.slots.len()).filter(|&i| !self.slots[i].matched && rows(self.slots[i].track.state)));
        a.cols.extend((0..dets.len()).filter(|&i| !self.det_used[i] && cols(&dets[i])));
        let (slots, hist, w) = (&self.slots, &self.det_hist, self.cfg.appearance_weight);
        a.solve(|si, di| {
            let s = &slots[si];
            let iou = s.kf.rect().iou(&dets[di].rect);
            if iou < min_iou {
                return None;
            }
            let mut cost = 1.0 - iou;
            if let (true, Some(h)) = (colour && !s.hist.is_empty(), hist.get(di)) {
                cost += w * (1.0 - s.hist.similarity(h));
            }
            Some(cost as f64)
        });
        for k in 0..self.assoc.pairs.len() {
            let (si, di) = self.assoc.pairs[k];
            self.matched(si, &dets[di], di, t);
        }
    }

    /// Every confident detection still unmatched either brings back a lost
    /// track that looks like it, or starts a new one.
    pub(super) fn start_new(&mut self, dets: &[Detection], t: Instant) {
        for (di, d) in dets.iter().enumerate() {
            if self.det_used[di] || d.score < self.cfg.new_track_score {
                continue;
            }
            match self.reidentify(d, di) {
                Some(si) => self.matched(si, d, di, t),
                None => self.spawn(d, di, t),
            }
        }
    }

    /// The lost track this detection most likely is. With colour: alike
    /// enough and within `reid_distance`. Without: a similar size, close by.
    fn reidentify(&self, d: &Detection, di: usize) -> Option<usize> {
        let hist = self.det_hist.get(di).filter(|h| !h.is_empty());
        let mut best: Option<(usize, f32)> = None;
        for (si, s) in self.slots.iter().enumerate() {
            if s.matched || s.track.state != TrackState::Lost || s.track.class != d.class {
                continue;
            }
            let r = s.kf.rect();
            let size = d.rect.h / r.h.max(1e-3);
            let dist = r.center().distance(d.rect.center());
            if !(0.5..=2.0).contains(&size) {
                continue;
            }
            let score = match hist {
                Some(h) if !s.hist.is_empty() && dist <= self.cfg.reid_distance => s.hist.similarity(h),
                _ if dist <= r.h.max(d.rect.h) * 0.5 => self.cfg.reid_similarity,
                _ => continue,
            };
            if score >= self.cfg.reid_similarity && best.is_none_or(|(_, b)| score > b) {
                best = Some((si, score));
            }
        }
        best.map(|(si, _)| si)
    }
}
