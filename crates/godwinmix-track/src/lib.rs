//! The GodwinMix tracking engine: frames in as borrowed pixel slices, people
//! and camera commands out. Five layers, each usable alone:
//!
//! 1. [`detect`]: a [`Detector`] finds people in a [`FrameRef`].
//! 2. [`track`]: a [`Tracker`] gives each one a stable id, `person-3`.
//! 3. [`choose`]: a [`Chooser`] picks who to follow.
//! 4. [`framing`]: [`frame_target`] turns them into a shot for a [`Framing`].
//! 5. [`steer`]: [`PtzSteer`] gives camera speeds, [`CropSteer`] a crop.
//!
//! [`Follower`] runs all five on each frame. Coordinates are normalised to
//! 0..1 everywhere. Nothing here needs a media stack or an async runtime.
#![forbid(unsafe_code)]

pub mod choose;
pub mod detect;
pub mod follow;
pub mod frame;
pub mod framing;
pub mod geom;
pub mod steer;
pub mod track;

pub use choose::{Choice, Choose, ChooseConfig, Chooser, Fallback, FollowState};
pub use detect::motion::{MotionConfig, MotionDetector};
pub use detect::{Class, Detection, Detector, Joint, Keypoint, Keypoints};
pub use follow::{FollowConfig, FollowOutput, Follower};
pub use frame::{FrameError, FrameRef, PixelFormat};
pub use framing::{frame_target, FrameStyle, Framing};
pub use geom::{Point, Rect};
pub use steer::{CropConfig, CropSteer, PtzConfig, PtzSteer, PtzVelocity};
pub use track::{Track, TrackId, TrackState, Tracker, TrackerConfig};
