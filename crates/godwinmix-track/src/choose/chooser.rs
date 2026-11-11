//! The chooser: the policy applied frame by frame, with stickiness and the
//! hold on a lost target.

use std::time::Instant;

use super::{Choice, Choose, ChooseConfig, FollowState};
use crate::geom::{Point, Rect};
use crate::track::{Track, TrackId, TrackState};

/// Keeps the current choice between frames.
#[derive(Clone, Debug)]
pub struct Chooser {
    pub policy: Choose,
    pub config: ChooseConfig,
    current: Option<TrackId>,
    last_rect: Option<Rect>,
    missing_since: Option<Instant>,
}

impl Chooser {
    pub fn new(policy: Choose, config: ChooseConfig) -> Self {
        Self { policy, config, current: None, last_rect: None, missing_since: None }
    }

    /// Changes the policy and lets the next frame choose afresh.
    pub fn set_policy(&mut self, policy: Choose) {
        self.policy = policy;
        self.current = match policy {
            Choose::Locked(id) => Some(id),
            _ => None,
        };
        self.missing_since = None;
    }

    pub fn current(&self) -> Option<TrackId> {
        self.current
    }

    pub fn choose(&mut self, tracks: &[Track], t: Instant) -> Choice {
        let seen = |id| tracks.iter().find(|tr| tr.id == id && tr.state == TrackState::Confirmed);
        if let Some(tr) = self.current.and_then(seen) {
            if let Some(better) = self.better_than(tr, tracks) {
                self.current = Some(better.id);
                return self.following(better);
            }
            return self.following(tr);
        }
        if self.current.is_some() {
            let since = *self.missing_since.get_or_insert(t);
            if t.saturating_duration_since(since) < self.config.lost_hold {
                let rect = self.current.and_then(|id| tracks.iter().find(|tr| tr.id == id)).map(|tr| tr.rect);
                let rect = rect.or(self.last_rect);
                return Choice { target: self.current, rect, state: FollowState::Holding };
            }
        }
        let best = match self.policy {
            Choose::Locked(_) => None,
            _ => self.best(tracks),
        };
        match best {
            Some(tr) => {
                self.current = Some(tr.id);
                self.following(tr)
            }
            None if self.missing_since.is_some() => {
                let state = FollowState::Fallback(self.config.fallback.clone());
                Choice { target: None, rect: None, state }
            }
            None => Choice { target: None, rect: None, state: FollowState::Idle },
        }
    }

    fn following(&mut self, tr: &Track) -> Choice {
        self.missing_since = None;
        self.last_rect = Some(tr.rect);
        Choice { target: Some(tr.id), rect: Some(tr.rect), state: FollowState::Following }
    }

    fn better_than<'a>(&self, cur: &Track, tracks: &'a [Track]) -> Option<&'a Track> {
        if matches!(self.policy, Choose::Locked(_)) {
            return None;
        }
        let best = self.best(tracks)?;
        let margin = self.config.switch_margin;
        (best.id != cur.id && self.score(best, tracks) > self.score(cur, tracks) * margin).then_some(best)
    }

    fn best<'a>(&self, tracks: &'a [Track]) -> Option<&'a Track> {
        tracks
            .iter()
            .filter(|tr| tr.state == TrackState::Confirmed)
            .max_by(|a, b| self.score(a, tracks).total_cmp(&self.score(b, tracks)))
    }

    /// Higher is better, always positive.
    fn score(&self, tr: &Track, tracks: &[Track]) -> f32 {
        let near = |p: Point| 1.0 / (0.05 + tr.rect.center().distance(p));
        match self.policy {
            Choose::Locked(id) => (tr.id == id) as u8 as f32,
            Choose::Largest => {
                let max = tracks.iter().map(|o| o.rect.area()).fold(1e-6, f32::max);
                tr.rect.area() / max + 1e-6
            }
            Choose::Central => near(Point::new(0.5, 0.5)),
            Choose::Nearest(p) => near(p),
            Choose::Speaker { x } => 1.0 / (0.05 + (tr.rect.cx() - x).abs()),
        }
    }
}
