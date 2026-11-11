//! The whole chain in one call: detect (every Nth frame), track, choose,
//! frame and steer. What a PTZ follow loop or a virtual camera runs.

mod control;
mod follower;

pub use follower::Follower;

use crate::choose::{Choose, ChooseConfig, FollowState};
use crate::ego::{CameraOptics, EgoMotion};
use crate::framing::FrameStyle;
use crate::geom::Rect;
use crate::steer::{CropConfig, PtzConfig, PtzVelocity};
use crate::track::{Track, TrackId, TrackerConfig};

/// Everything the follower can be tuned with. The defaults suit a person
/// speaking on a stage, seen at 320 pixels wide and 10 to 30 frames a second.
#[derive(Clone, Debug, PartialEq)]
pub struct FollowConfig {
    /// Run the detector on one frame in this many; the tracker coasts on
    /// the others. 1 runs it on every frame.
    pub detect_every: u32,
    pub policy: Choose,
    pub tracker: TrackerConfig,
    pub choose: ChooseConfig,
    pub style: FrameStyle,
    pub ptz: PtzConfig,
    pub crop: CropConfig,
    /// A PTZ camera's optics, so the follower knows how its own commands
    /// move the picture (see [`crate::ego`]). `None` for a fixed camera or
    /// a digital crop, where the picture never moves.
    pub camera: Option<CameraOptics>,
}

impl Default for FollowConfig {
    fn default() -> Self {
        Self {
            detect_every: 1,
            policy: Choose::Largest,
            tracker: TrackerConfig::default(),
            choose: ChooseConfig::default(),
            style: FrameStyle::default(),
            ptz: PtzConfig::default(),
            crop: CropConfig::default(),
            camera: None,
        }
    }
}

/// One frame's result.
#[derive(Clone, Debug, PartialEq)]
pub struct FollowOutput {
    /// Who is being followed.
    pub target: Option<TrackId>,
    /// Every confirmed and lost track.
    pub tracks: Vec<Track>,
    /// Speeds for a PTZ camera.
    pub ptz: PtzVelocity,
    /// The smoothed crop for a digital camera, normalised, inside the frame.
    pub crop: Rect,
    /// The shot the steering is aiming for, before smoothing.
    pub shot: Option<Rect>,
    pub state: FollowState,
    /// True when the detector ran on this frame.
    pub detected: bool,
    /// How the camera moved the picture over this step.
    pub ego: EgoMotion,
}

impl Default for FollowOutput {
    fn default() -> Self {
        Self {
            target: None,
            tracks: Vec::with_capacity(32),
            ptz: PtzVelocity::STOP,
            crop: Rect::FULL,
            shot: None,
            state: FollowState::Idle,
            detected: false,
            ego: EgoMotion::STILL,
        }
    }
}
