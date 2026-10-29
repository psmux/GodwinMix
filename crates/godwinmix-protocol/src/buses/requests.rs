//! The params and answers of the `audio.*` methods that are not a bus's own
//! config.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn is_false(b: &bool) -> bool {
    !*b
}

/// `audio.master.set`. Only what is named moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct MasterSetRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

/// `audio.bus.set`. Only what is named moves. `device: ""` stops playing it
/// on a sound card, and `minus: ""` makes a mix minus bus an ordinary one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct BusSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minus: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

/// `audio.bus.remove`, and anything else that names one bus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BusIdRequest {
    pub id: String,
}

/// `audio.send.set`: how much of one source goes to one bus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SendSetRequest {
    pub bus: String,
    pub source: String,
    /// 0.0 takes the source off the bus, 1.0 is unity, up to 10.0.
    pub level: f64,
}

/// `audio.cue.set`: mark or clear one source for the cue bus.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CueSetRequest {
    pub source: String,
    pub cue: bool,
    /// Clear every other mark first, so only this source is heard: solo.
    #[serde(default, skip_serializing_if = "is_false")]
    pub solo: bool,
}

/// One sound card a bus can be played on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioDevice {
    /// Its number in this list, which `device` also takes.
    pub index: usize,
    /// The id `device` takes, the most stable thing the platform says about it.
    pub id: String,
    pub name: String,
    /// The platform's sound API, when it says: `osxaudio`, `wasapi2`,
    /// `pulse`, `alsa`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api: Option<String>,
}

/// `audio.device.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct AudioDeviceList {
    pub devices: Vec<AudioDevice>,
}
