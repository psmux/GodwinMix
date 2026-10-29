//! Audio buses: the programme master, the named mixes beside it, cue, and the
//! local devices a bus can be played on.
//!
//! The same types are the config file's `[audio]` table and the wire shape of
//! the `audio.*` methods, so a bus written by hand and one added from the desk
//! read back the same way.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

mod requests;
mod status;
pub use requests::*;
pub use status::*;

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

#[cfg(test)]
mod tests;
