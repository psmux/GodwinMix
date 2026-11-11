//! Each track's life: matched, confirmed, lost, retired, or newly started.

use std::time::Instant;

use super::tracker::Slot;
use super::{BoxFilter, Track, TrackId, TrackState, Tracker};
use crate::detect::Detection;
use crate::geom::Point;

impl Tracker {
    pub(super) fn matched(&mut self, slot: usize, det: &Detection, di: usize, t: Instant) {
        self.det_used[di] = true;
        let s = &mut self.slots[slot];
        s.matched = true;
        let before = s.kf.rect().center();
        s.kf.update(det.rect);
        if let Some(h) = self.det_hist.get(di) {
            s.hist.blend(h, 0.1);
        }
        let tr = &mut s.track;
        tr.hits += 1;
        tr.score = det.score;
        tr.class = det.class;
        // A detector that adds keypoints only now and then: carry the last
        // ones along with the box until fresh ones come.
        let (dx, dy) = (det.rect.cx() - before.x, det.rect.cy() - before.y);
        tr.keypoints = det.keypoints.or(tr.keypoints.map(|mut k| {
            k.0.iter_mut().for_each(|p| (p.at.x, p.at.y) = (p.at.x + dx, p.at.y + dy));
            k
        }));
        tr.last_seen = t;
        if tr.state == TrackState::Lost || tr.hits >= self.cfg.confirm_hits {
            tr.state = TrackState::Confirmed;
        }
    }

    pub(super) fn retire(&mut self, t: Instant) {
        let max_lost = self.cfg.max_lost;
        self.slots.retain_mut(|s| {
            if s.matched {
                return true;
            }
            match s.track.state {
                TrackState::Tentative => false,
                TrackState::Confirmed => {
                    s.track.state = TrackState::Lost;
                    true
                }
                TrackState::Lost => t.saturating_duration_since(s.track.last_seen) <= max_lost,
            }
        });
    }

    pub(super) fn spawn(&mut self, det: &Detection, di: usize, t: Instant) {
        // As in ByteTrack, the people in the very first frame are trusted
        // at once, so following can start without waiting a frame.
        let state = if self.cfg.confirm_hits <= 1 || self.frames <= 1 {
            TrackState::Confirmed
        } else {
            TrackState::Tentative
        };
        let track = Track {
            id: TrackId(self.next_id),
            rect: det.rect,
            velocity: Point::default(),
            score: det.score,
            class: det.class,
            keypoints: det.keypoints,
            state,
            hits: 1,
            first_seen: t,
            last_seen: t,
        };
        self.next_id += 1;
        let hist = self.det_hist.get(di).copied().unwrap_or_default();
        self.slots.push(Slot { track, kf: BoxFilter::new(det.rect, self.cfg.kalman), hist, matched: true });
    }
}
