//! Layer three: who to follow. A policy picks among the confirmed tracks, the
//! choice sticks unless somebody is clearly better, and losing the target
//! holds the shot for a while before falling back.

mod chooser;

pub use chooser::Chooser;

use std::sync::Arc;
use std::time::Duration;

use crate::geom::{Point, Rect};
use crate::track::TrackId;

/// How to pick the person to follow.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Choose {
    /// This id and nobody else.
    Locked(TrackId),
    /// The biggest box, usually the nearest person.
    Largest,
    /// The one nearest the middle of the frame.
    Central,
    /// The one nearest a point, such as where the operator tapped.
    Nearest(Point),
    /// The one nearest the horizontal position `x`, 0 to 1, where the audio
    /// says the voice is coming from.
    Speaker { x: f32 },
}

/// What to do once a lost target has been held for `lost_hold`.
#[derive(Clone, Debug, PartialEq)]
pub enum Fallback {
    /// Keep the last shot.
    Stay,
    /// Go wide: the whole frame for a crop, zoomed out for a PTZ camera.
    Wide,
    /// Go to this camera preset; the PTZ host does the move.
    Preset(Arc<str>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChooseConfig {
    /// How long to hold the shot on a target that has vanished.
    pub lost_hold: Duration,
    pub fallback: Fallback,
    /// Another person must score this many times the current one before the
    /// choice moves to them, so it does not flick between two.
    pub switch_margin: f32,
}

impl Default for ChooseConfig {
    fn default() -> Self {
        Self { lost_hold: Duration::from_secs(2), fallback: Fallback::Stay, switch_margin: 1.5 }
    }
}

/// What the follower is doing, in words a window can show.
#[derive(Clone, Debug, PartialEq)]
pub enum FollowState {
    /// Nobody to follow, or following is stopped.
    Idle,
    /// The target is in view.
    Following,
    /// The target has just vanished; the shot holds where they were.
    Holding,
    /// The target has been gone longer than the hold.
    Fallback(Fallback),
}

/// The chooser's answer for one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub target: Option<TrackId>,
    /// Where the target is, or was last seen while holding.
    pub rect: Option<Rect>,
    pub state: FollowState,
}
