//! The follower itself: the five layers, run in order on every frame.

use std::time::Instant;

use super::{FollowConfig, FollowOutput};
use crate::choose::{Choice, Chooser, Fallback, FollowState};
use crate::detect::{Detection, Detector};
use crate::ego::EgoMotion;
use crate::frame::FrameRef;
use crate::framing::frame_target;
use crate::geom::Rect;
use crate::steer::{CropSteer, PtzSteer, PtzVelocity};
use crate::track::{Track, Tracker};

/// Detect, track, choose, frame and steer.
pub struct Follower<D = Box<dyn Detector + Send>> {
    pub(super) detector: D,
    pub(super) config: FollowConfig,
    pub(super) tracker: Tracker,
    pub(super) chooser: Chooser,
    pub(super) ptz: PtzSteer,
    crop: CropSteer,
    dets: Vec<Detection>,
    pub(super) frames: u64,
    pub(super) active: bool,
    /// What the camera was last told, from outside; see `set_camera_velocity`.
    pub(super) moving: Option<PtzVelocity>,
    /// What this follower last told it.
    last_cmd: PtzVelocity,
    /// How far the camera has zoomed since the follow started, from the
    /// zoom speeds it was told: the optics' field of view is for 1.
    pub(super) zoom_scale: f32,
    last_t: Option<Instant>,
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
            moving: None,
            last_cmd: PtzVelocity::STOP,
            zoom_scale: 1.0,
            last_t: None,
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
        let ego = self.ego(t);
        let tracks = if out.detected {
            self.detector.camera_motion(ego);
            self.detector.detect_into(frame, &mut self.dets);
            self.tracker.update_with_camera(&self.dets, Some(frame), t, ego)
        } else {
            self.tracker.coast_with_camera(t, ego)
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
        out.ego = ego;
        self.last_cmd = out.ptz;
    }

    /// The camera's motion over the step to `t`: what it was told last,
    /// through the optics, when the config has them. A camera zoomed in
    /// twice as far sees a pan move its picture twice as fast.
    fn ego(&mut self, t: Instant) -> EgoMotion {
        let told = self.moving.take().unwrap_or(self.last_cmd);
        let dt = self.last_t.map_or(0.0, |l| t.saturating_duration_since(l).as_secs_f32());
        self.last_t = Some(t);
        let Some(optics) = self.config.camera else { return EgoMotion::STILL };
        let e = optics.ego(told);
        let e = EgoMotion { dx: e.dx * self.zoom_scale, dy: e.dy * self.zoom_scale, zoom: e.zoom };
        self.zoom_scale = (self.zoom_scale * (e.zoom * dt).exp()).clamp(0.05, 50.0);
        e
    }

    fn shot(&self, choice: &Choice, tracks: &[Track], aspect: f32) -> Option<Rect> {
        let id = choice.target?;
        let mut track = *tracks.iter().find(|t| t.id == id)?;
        if let Some(r) = choice.rect {
            track.rect = r;
        }
        Some(frame_target(&track, &self.config.style, aspect))
    }
}
