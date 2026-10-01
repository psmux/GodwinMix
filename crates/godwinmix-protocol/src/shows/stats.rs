//! The numbers `show.stats` reads: an input's and each output's.

use crate::health::Health;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What the input is doing, as the host last counted it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputStats {
    #[serde(default)]
    pub kbps: u32,
    #[serde(default)]
    pub fps: f64,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub video_codec: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub audio_channels: u32,
    #[serde(default)]
    pub cc_errors: u64,
    #[serde(default)]
    pub packets_lost: u64,
    /// Between the last two keyframes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyframe_ms: Option<u64>,
    /// Since the last frame arrived.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_frame_ms: Option<u64>,
}

/// What one output is doing.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputStats {
    pub id: String,
    /// waiting, connecting, live, reconnecting, failed, or off.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub kbps: u32,
    #[serde(default)]
    pub reconnects: u32,
    /// `copy`, or what the plan gave it, such as `h264 1280x720`.
    #[serde(default)]
    pub rendition_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encoder: Option<String>,
    #[serde(default)]
    pub cpu_millicores: u32,
}

/// What a show does to make its outputs, which is what its load pays for.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShowWork {
    /// It composites and encodes a programme of its own: a show that mixes.
    Mix,
    /// A direct show with an output given a rendition, which is encoded.
    Transcode,
    /// A direct show whose outputs all copy the input.
    #[default]
    Copy,
}

/// One show's numbers, as `show.stats` answers them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowStats {
    pub id: String,
    pub health: Health,
    /// `mix`, `transcode` or `copy`.
    #[serde(default)]
    pub work: ShowWork,
    /// What the show costs the machine, thousandths of a core: for a show
    /// that mixes, its process as last measured; for a direct show, its
    /// outputs' encodes summed. Left out while nothing has measured it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_millicores: Option<u32>,
    /// None for a show with no input, or before the host has counted any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputStats>,
    #[serde(default)]
    pub outputs: Vec<OutputStats>,
}
