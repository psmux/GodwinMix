//! The params of the `replay.*` methods.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

/// Which point `replay.mark` sets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum MarkPoint {
    /// Open a new event here.
    In,
    /// Close the open event here.
    Out,
}

/// `replay.mark`. With `point` it sets an in or an out point; with
/// `seconds` alone it makes a whole event of the last that many seconds.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MarkRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub point: Option<MarkPoint>,
    /// For an in point, how many seconds before now it goes. For a whole
    /// event, how long it is. 0 is now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// A name for the event. Its id when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The angle it was seen on, which playback starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
}

/// `replay.event.rename`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventRenameRequest {
    pub id: String,
    pub name: String,
}

/// `replay.event.delete`, and anything else that names one event.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EventIdRequest {
    pub id: String,
}

/// `replay.play`: load an event, or the last seconds of an angle, into a
/// replay source and play it from the start.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct PlayRequest {
    /// The event to play. Without one, the last `seconds` of the angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// How far back to start when no event is named. 10 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
    /// The angle to show. The event's own angle, then the first angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
    /// 1.0 is real time, 0.5 half speed, down to 0.1. 1.0 when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    /// The source to play it on, added if it is not there. `replay` when
    /// left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Put it on air once it is loaded, as `program.take` would.
    #[serde(default, skip_serializing_if = "is_false")]
    pub take: bool,
    /// The transition for that take: a name such as `"fade"` or a stinger's
    /// object. A cut when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<crate::requests::Transition>,
}

/// `replay.set`: change the angle or the speed of a replay that is playing,
/// from the frame it is on.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ReplaySetRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
}

/// `replay.save`: write an event, or the whole buffer of one angle, to an
/// MP4 in the media folder without encoding it again.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct SaveRequest {
    /// The event to save. The whole buffer when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// The angle to save. The event's own, then `programme` if it is
    /// recorded, then the first angle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<String>,
    /// The file's name, without a folder. Made from the time when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}
