//! Layer two: follow detections from frame to frame and give each person a
//! stable id. ByteTrack in shape: a Kalman filter per track, IoU matching
//! with the Hungarian method, a pass for confident detections and one for
//! weak ones, and a colour histogram to bring back a person who was hidden.

mod appearance;
mod assoc;
mod hungarian;
mod kalman;
mod life;
mod passes;
mod tracker;

pub use appearance::Histogram;
pub use hungarian::Hungarian;
pub use kalman::{BoxFilter, KalmanConfig};
pub use tracker::Tracker;

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use crate::detect::{Class, Keypoints};
use crate::geom::{Point, Rect};

/// A track's id. Shown and parsed as a slug, `person-3`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TrackId(pub u32);

impl fmt::Display for TrackId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "person-{}", self.0)
    }
}

impl FromStr for TrackId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.strip_prefix("person-")
            .and_then(|n| n.parse().ok())
            .map(TrackId)
            .ok_or_else(|| format!("{s:?} is not a track id; ids look like person-3, as track/observe lists them"))
    }
}

/// Where a track is in its life.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TrackState {
    /// Seen once, not yet trusted. Never in [`Tracker::tracks`].
    Tentative,
    /// Seen this frame or recently enough to be trusted.
    Confirmed,
    /// Not seen for a while. The box is where the filter thinks it went.
    Lost,
}

/// One person (or moving thing) being followed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Track {
    pub id: TrackId,
    /// The filtered box, normalised.
    pub rect: Rect,
    /// How fast the centre moves, normalised units per second.
    pub velocity: Point,
    /// The score of the last detection matched.
    pub score: f32,
    pub class: Class,
    pub keypoints: Option<Keypoints>,
    pub state: TrackState,
    /// How many detections have been matched to this track.
    pub hits: u32,
    pub first_seen: Instant,
    pub last_seen: Instant,
}

impl Track {
    /// The id as a slug, `person-3`.
    pub fn slug(&self) -> String {
        self.id.to_string()
    }

    pub fn is_confirmed(&self) -> bool {
        self.state == TrackState::Confirmed
    }
}

/// The tracker's thresholds. The defaults suit 10 to 30 detections a second.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrackerConfig {
    /// A detection at or above this is confident and may start a track.
    pub high_score: f32,
    /// Below this a detection is ignored. Between the two it can only keep
    /// an existing track alive.
    pub low_score: f32,
    /// A confident detection needs this much to start a new track.
    pub new_track_score: f32,
    /// The least IoU for a confident match.
    pub match_iou: f32,
    /// The least IoU for a weak detection to keep a track.
    pub low_match_iou: f32,
    /// Matches a track needs before it is confirmed.
    pub confirm_hits: u32,
    /// How long a lost track is kept, and so can be brought back.
    pub max_lost: Duration,
    /// How alike two colour histograms must be to bring a track back, 0 to 1.
    pub reid_similarity: f32,
    /// How far, in normalised units, a person may reappear from where they
    /// were lost and still get their id back by colour.
    pub reid_distance: f32,
    /// How much colour counts against IoU when matching, 0 to 1.
    pub appearance_weight: f32,
    pub kalman: KalmanConfig,
}

impl Default for TrackerConfig {
    fn default() -> Self {
        Self {
            high_score: 0.5,
            low_score: 0.1,
            new_track_score: 0.6,
            match_iou: 0.2,
            low_match_iou: 0.4,
            confirm_hits: 2,
            max_lost: Duration::from_secs(5),
            reid_similarity: 0.75,
            reid_distance: 0.6,
            appearance_weight: 0.3,
            kalman: KalmanConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_slugs_both_ways() {
        assert_eq!(TrackId(3).to_string(), "person-3");
        assert_eq!("person-12".parse::<TrackId>(), Ok(TrackId(12)));
        assert!("12".parse::<TrackId>().unwrap_err().contains("person-3"));
    }
}
