//! Audio buses: the programme master, the named mixes beside it, cue, and the
//! local devices a bus can be played on.
//!
//! The same types are the config file's `[audio]` table and the wire shape of
//! the `audio.*` methods, so a bus written by hand and one added from the desk
//! read back the same way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The loudest a master or a send can be set to: the same ceiling a source's
/// fader has.
pub const MAX_LEVEL: f64 = 10.0;

fn one() -> f64 {
    1.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_one(v: &f64) -> bool {
    (*v - 1.0).abs() < f64::EPSILON
}

/// The `[audio]` table: the programme's master and every bus.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioConfig {
    #[serde(default, skip_serializing_if = "MasterConfig::is_default")]
    pub master: MasterConfig,
    /// One `[[audio.bus]]` per bus. None by default, and a programme with none
    /// builds nothing for them.
    #[serde(default, rename = "bus", skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<BusConfig>,
}

impl AudioConfig {
    pub fn is_default(&self) -> bool {
        self == &AudioConfig::default()
    }
}

/// A master fader and mute.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MasterConfig {
    /// 0.0 silent, 1.0 unity, up to 10.0.
    #[serde(default = "one")]
    pub gain: f64,
    #[serde(default)]
    pub muted: bool,
}

impl Default for MasterConfig {
    fn default() -> Self {
        Self { gain: 1.0, muted: false }
    }
}

impl MasterConfig {
    pub fn is_default(&self) -> bool {
        self == &MasterConfig::default()
    }
}

/// One bus: a mix of its own beside the programme.
///
/// Three kinds, decided by two keys. With neither, it carries each source at
/// the level its `sends` give it (a source with no send is not on it). With
/// `minus`, it is the programme as heard, without that one source: a guest's
/// return. With `cue`, it carries the sources marked cue at unity, and the
/// programme while nothing is marked.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BusConfig {
    /// A slug: `monitor`, `guest-return`, `recording-b`.
    pub id: String,
    /// What the desk shows. The id when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The bus master, 0.0 to 10.0.
    #[serde(default = "one", skip_serializing_if = "is_one")]
    pub gain: f64,
    #[serde(default, skip_serializing_if = "is_false")]
    pub muted: bool,
    /// Level per source id, 0.0 to 10.0, after the source's own fader and
    /// mute. Read only on a bus with neither `minus` nor `cue`.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sends: BTreeMap<String, f64>,
    /// Make this bus the programme without this source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minus: Option<String>,
    /// Make this the cue bus.
    #[serde(default, skip_serializing_if = "is_false")]
    pub cue: bool,
    /// Play the bus on a sound card here: `default`, a device name, its id,
    /// or its number in `audio.device.list`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
}

impl BusConfig {
    pub fn named(id: &str) -> Self {
        Self {
            id: id.to_string(),
            name: None,
            gain: 1.0,
            muted: false,
            sends: BTreeMap::new(),
            minus: None,
            cue: false,
            device: None,
        }
    }

    /// What kind of mix this is, as `audio.bus.list` names it.
    pub fn kind(&self) -> &'static str {
        if self.cue {
            "cue"
        } else if self.minus.is_some() {
            "minus"
        } else {
            "sends"
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bus_written_by_hand_reads_back_the_same() {
        let text = r#"
            [master]
            gain = 0.8

            [[bus]]
            id = "guest-return"
            minus = "guest-1"

            [[bus]]
            id = "monitor"
            sends = { cam1 = 0.5 }
        "#;
        let cfg: AudioConfig = toml::from_str(text).unwrap();
        assert_eq!(cfg.master.gain, 0.8);
        assert_eq!(cfg.buses[0].kind(), "minus");
        assert_eq!(cfg.buses[1].kind(), "sends");
        assert_eq!(cfg.buses[1].sends["cam1"], 0.5);
        let again: AudioConfig = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(cfg, again);
    }
}
