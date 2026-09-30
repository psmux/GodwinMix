//! What a destination that asked for a rendition was given: a copy or an
//! encode, and why; or a refusal with what would fit instead.
//!
//! Every field is left out of a destination that asked for nothing, so a
//! plain copy destination reads exactly as it did before renditions.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::rendition::{AudioShape, Cost, RenditionRequest, VideoShape};

/// Copied as it arrives, or converted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum DestinationMode {
    Copy,
    Transcode,
}

/// The plan's answer for one destination.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DestinationPlan {
    pub mode: DestinationMode,
    /// The stream it was planned against, when the destination names `*`.
    pub stream: String,
    /// One sentence: "copied: the source's video goes out as it is", "encoded
    /// because the source is 1920x1080 and this output wants 1280x720".
    pub reason: String,
    /// The video encoder, `h264-videotoolbox`, and why that one. Absent for a
    /// copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder_reason: Option<String>,
    /// What goes out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video: Option<VideoShape>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioShape>,
    /// The plan's nodes this destination reads, so a page can show which
    /// work it shares with the channel's other destinations.
    pub nodes: Vec<String>,
}

/// One thing the page can offer as a button: retry with this request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RenditionAdvice {
    /// "720p30 H.264 on h264-software-x264".
    pub text: String,
    pub request: RenditionRequest,
}

/// Why a destination that asked for a rendition is not sending, and what
/// would. `error` on the destination carries the same sentence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DestinationRefusal {
    /// `governor` (the machine has no room), `plan` (nothing here can make
    /// it), `shed` (it ran and was stopped to keep what is on air).
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub need: Option<Cost>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub have: Option<Cost>,
    /// Renditions that would fit now, largest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub advice: Vec<RenditionAdvice>,
}
