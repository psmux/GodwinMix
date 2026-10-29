//! What the `replay.*` methods answer with.

use super::{ReplayConfig, ReplayEvent};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One angle's ring, as it stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AngleStatus {
    /// The source id, or `programme`.
    pub id: String,
    /// Seconds of picture held, from the oldest keyframe to the newest frame.
    pub held_secs: f64,
    /// Bytes of encoded picture and sound held.
    pub bytes: u64,
    /// Whole groups of pictures held. The ring drops the oldest one at a time.
    pub gops: u32,
    /// The encoder this angle is written with, or `shared` when it reads the
    /// programme encoder's own output.
    pub encoder: String,
    /// Frames the angle's encoder could not keep up with and dropped. The
    /// source and the programme lose none.
    pub dropped: u64,
    /// Why this angle holds nothing, when it does not: the source is not
    /// there, or has not sent a picture yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub waiting: Option<String>,
}

/// `replay.get`: everything about replay on this mixer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ReplayStatus {
    /// Whether anything is being recorded.
    pub on: bool,
    pub config: ReplayConfig,
    pub angles: Vec<AngleStatus>,
    /// Every event, oldest first. An event whose stretch has left every ring
    /// is still listed until it is deleted, and plays nothing.
    pub events: Vec<ReplayEvent>,
    /// The mixer's clock now, in the same milliseconds as an event's points.
    pub now_ms: u64,
}

/// What `replay.play` and `replay.set` answer with.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayStatus {
    /// The source it plays on.
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    pub angle: String,
    pub speed: f64,
    /// How long it plays at this speed.
    pub duration_ms: u64,
    /// Where it is, in milliseconds of playback.
    pub position_ms: u64,
}

/// What `replay.save` answers with once the file is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SaveStatus {
    /// The file, as the media library names it: `Replays/<name>.mp4`.
    pub name: String,
    /// Its absolute path on the mixer's machine.
    pub path: String,
    pub angle: String,
    pub duration_ms: u64,
    pub bytes: u64,
}
