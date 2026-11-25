//! The desk as `audio.bus.list` and the status snapshot report it.

use super::{BusConfig, MasterConfig};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The audio desk as `audio.bus.list` and the status snapshot report it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioStatus {
    pub master: MasterConfig,
    pub buses: Vec<BusStatus>,
    /// The sources marked cue, in the order they were marked.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cue: Vec<String>,
}

/// One bus and what is using it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BusStatus {
    #[serde(flatten)]
    pub config: BusConfig,
    /// `sends`, `minus` or `cue`.
    pub kind: String,
    /// The outputs that take their sound from this bus.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<String>,
    /// The device it is playing on, once it opened, or why it is not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_state: Option<String>,
    /// Where to hear it in a browser: `/pcm/bus:<id>` or `/opus/bus:<id>`.
    pub listen: String,
}
