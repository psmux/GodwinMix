//! The follower itself: the five layers, run in order on every frame.

use std::time::Instant;

use super::{FollowConfig, FollowOutput};
use crate::choose::{Choice, Choose, Chooser, Fallback, FollowState};
use crate::detect::{Detection, Detector};
use crate::frame::FrameRef;
use crate::framing::{frame_target, Framing};
use crate::geom::{Point, Rect};
use crate::steer::{CropSteer, PtzSteer};
use crate::track::{Track, TrackId, Tracker};

/// Detect, track, choose, frame and steer.
pub struct Follower<D = Box<dyn Detector + Send>> {
    detector: D,
    config: FollowConfig,
    tracker: Tracker,
    chooser: Chooser,
    ptz: PtzSteer,
    crop: CropSteer,
    dets: Vec<Detection>,
    frames: u64,
    active: bool,
}

impl<D: Detector> Follower<D> {
    pub fn new(detector: D, config: FollowConfig) -> Self {
        Self {
            tracker: Tracker::new(config.tracker),
            chooser: Chooser::new(config.policy, config.choose.clone()),
            ptz: PtzSteer::new(config.ptz),
            crop: CropSteer::new(config.crop),
            detector,
            config,
            dets: Vec::with_capacity(64),
            frames: 0,
            active: true,
        }
    }

    /// Runs one frame taken at `t`. Allocates the output's track list; a
    /// loop that keeps one [`FollowOutput`] should call
    /// [`step_into`](Follower::step_into) instead.
    pub fn step(&mut self, frame: &FrameRef, t: Instant) -> FollowOutput {
        let mut out = FollowOutput::default();
        self.step_into(frame, t, &mut out);
        out
    }

    /// Runs one frame into `out`, reusing its buffers: nothing allocates
    /// once the vectors have grown to the number of people in view.
    pub fn step_into(&mut self, frame: &FrameRef, t: Instant, out: &mut FollowOutput) {
        let every = self.config.detect_every.max(1) as u64;
        out.detected = self.frames % every == 0 && frame.validate().is_ok();
        self.frames += 1;
        let tracks = if out.detected {
            self.detector.detect_into(frame, &mut self.dets);
            self.tracker.update_with_frame(&self.dets, Some(frame), t)
        } else {
            self.tracker.coast(t)
        };
        out.tracks.clear();
        out.tracks.extend_from_slice(tracks);
        let choice = match self.active {
            true => self.chooser.choose(&out.tracks, t),
            false => Choice { target: None, rect: None, state: FollowState::Idle },
        };
        let aspect = if frame.height > 0 { frame.aspect() } else { 16.0 / 9.0 };
        let shot = self.shot(&choice, &out.tracks, aspect);
        out.ptz = self.ptz.step(shot.filter(|_| choice.state != FollowState::Holding), t);
        let crop_goal = match &choice.state {
            FollowState::Fallback(Fallback::Wide) | FollowState::Fallback(Fallback::Preset(_)) => Some(Rect::FULL),
            _ => shot,
        };
        out.crop = self.crop.step(crop_goal, t);
        out.target = choice.target;
        out.shot = shot;
        out.state = choice.state;
    }

    fn shot(&self, choice: &Choice, tracks: &[Track], aspect: f32) -> Option<Rect> {
        let id = choice.target?;
        let mut track = *tracks.iter().find(|t| t.id == id)?;
        if let Some(r) = choice.rect {
            track.rect = r;
        }
        Some(frame_target(&track, &self.config.style, aspect))
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
    }
}
