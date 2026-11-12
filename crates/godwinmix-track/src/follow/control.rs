//! Steering a running follower: who to follow, the framing, stop, reset,
//! and what the camera is really doing.

use super::Follower;
use crate::choose::Choose;
use crate::detect::Detector;
use crate::framing::Framing;
use crate::geom::Point;
use crate::steer::PtzVelocity;
use crate::track::{TrackId, Tracker};
use super::FollowConfig;

impl<D: Detector> Follower<D> {
    /// What the camera is moving at now, when something besides this
    /// follower moves it (an operator's joystick) or it does not do quite
    /// what it is told. Used for the next step only; without it the
    /// follower takes the camera to do what it last said.
    pub fn set_camera_velocity(&mut self, v: PtzVelocity) {
        self.moving = Some(v);
    }

    /// Follows this id only.
    pub fn follow(&mut self, id: TrackId) {
        self.set_policy(Choose::Locked(id));
    }

    /// Follows whoever is nearest `p` now, and keeps them.
    pub fn follow_point(&mut self, p: Point) {
        let near = self.tracker.tracks().iter().filter(|t| t.is_confirmed()).min_by(|a, b| {
            a.rect.center().distance(p).total_cmp(&b.rect.center().distance(p))
        });
        match near {
            Some(t) => self.follow(t.id),
            None => self.set_policy(Choose::Nearest(p)),
        }
    }

    pub fn set_policy(&mut self, policy: Choose) {
        self.active = true;
        self.config.policy = policy;
        self.chooser.set_policy(policy);
    }

    /// Changes the follower's settings while it runs: the framing style,
    /// the steering and the choosing all take the new values on the next
    /// frame, and nothing it is tracking is lost.
    pub fn retune(&mut self, edit: impl FnOnce(&mut FollowConfig)) {
        edit(&mut self.config);
        self.ptz.config = self.config.ptz;
        self.chooser.config = self.config.choose.clone();
    }

    pub fn set_framing(&mut self, framing: Framing) {
        self.config.style.framing = framing;
    }

    /// Stops following: tracks are still reported, the camera eases to a
    /// stop and the crop holds.
    pub fn stop(&mut self) {
        self.active = false;
    }

    pub fn config(&self) -> &FollowConfig {
        &self.config
    }

    pub fn tracker(&self) -> &Tracker {
        &self.tracker
    }

    pub fn detector_mut(&mut self) -> &mut D {
        &mut self.detector
    }

    /// Forgets every track and stops, for a cut or a preset move.
    pub fn reset(&mut self) {
        self.tracker.reset();
        self.detector.reset();
        self.ptz.reset();
        self.chooser.set_policy(self.config.policy);
        self.frames = 0;
        self.zoom_scale = 1.0;
    }
}
