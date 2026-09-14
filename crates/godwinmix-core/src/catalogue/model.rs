//! The shapes `codecs.toml` parses into.
//!
//! Every field here is optional except the ones an entry cannot mean anything
//! without, because the catalogue is edited by people with hardware rather
//! than by people with the source, and a missing `comment` should never be a
//! parse error. What is genuinely required is checked by [`Catalogue::validate`]
//! and reported with the entry's id, which is a better error than serde's.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A property value, either written out or derived from the running
/// configuration. `bitrate = 4000` is written out; `bitrate = { unit = "bit",
/// from = "video.bitrate_kbps" }` is derived, and the loader converts the
/// configured kilobits into the bits this element wants.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(untagged)]
pub enum PropValue {
    /// Order matters: a table only ever matches this arm.
    Derived(Derived),
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Derived {
    /// The unit this element's property is in.
    pub unit: String,
    /// The variable to take the value from. See `UNITS` in `apply.rs`.
    pub from: String,
}

/// Where the keyframe interval goes on this encoder, and in what unit.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    pub property: String,
    #[serde(default = "frames")]
    pub unit: String,
}

fn frames() -> String {
    "frames".into()
}

/// One report from somebody who ran `gmx codec test` on real hardware.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct Verified {
    #[serde(default)]
    pub platform: String,
    #[serde(default)]
    pub driver: String,
    #[serde(default)]
    pub gstreamer: String,
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub report: String,
}

/// What an entry is used for. An entry may offer both.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum Role {
    Encode,
    Decode,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Encode => "encode",
            Role::Decode => "decode",
        }
    }
}

/// A video codec entry. Encode and decode are selected independently, so an
/// entry may carry only one of `encoder` and `decoder`.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct VideoEntry {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub codec: String,
    #[serde(default = "software")]
    pub accel: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub encoder: Option<String>,
    #[serde(default)]
    pub decoder: Option<String>,
    /// Element that pulls frames from this backend's memory back into system
    /// memory. Hardware decoders hand out GPU surfaces and most of the graph
    /// works in system memory.
    #[serde(default)]
    pub download: Option<String>,
    /// The parser that goes between the encoder and a muxer.
    #[serde(default)]
    pub parser: Option<String>,
    /// The memory type this encoder accepts on its sink pad.
    #[serde(default = "system")]
    pub memory: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropValue>,
    #[serde(default)]
    pub keyframe: Option<Keyframe>,
    #[serde(default)]
    pub container: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub verified: Vec<Verified>,
    #[serde(default)]
    pub comment: Option<String>,
    /// Set in `[codecs]` to take a shipped entry out of selection without
    /// having to restate it.
    #[serde(default)]
    pub disabled: bool,
}

fn software() -> String {
    "software".into()
}

fn system() -> String {
    "system".into()
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct AudioEntry {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub codec: String,
    #[serde(default = "software")]
    pub accel: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub encoder: Option<String>,
    #[serde(default)]
    pub decoder: Option<String>,
    #[serde(default)]
    pub parser: Option<String>,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub properties: BTreeMap<String, PropValue>,
    #[serde(default)]
    pub container: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    /// Encoder delay this encoder does not take out of its own timestamps, so
    /// audio leaves it that much late relative to video. The programme holds
    /// the video back by it.
    #[serde(default)]
    pub priming_delay_ms: Option<u64>,
    #[serde(default)]
    pub verified: Vec<Verified>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

/// A compositor and conversion backend.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct GraphicsEntry {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default = "software")]
    pub accel: String,
    #[serde(default)]
    pub rank: i32,
    #[serde(default)]
    pub compositor: String,
    #[serde(default)]
    pub convert: String,
    #[serde(default)]
    pub upload: Option<String>,
    #[serde(default)]
    pub download: Option<String>,
    #[serde(default = "system")]
    pub memory: String,
    #[serde(default)]
    pub requires: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub verified: Vec<Verified>,
    #[serde(default)]
    pub comment: Option<String>,
    #[serde(default)]
    pub disabled: bool,
}

/// A container, its muxer, and what it can carry.
#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq)]
pub struct ContainerEntry {
    pub name: String,
    pub muxer: String,
    #[serde(default)]
    pub video: Vec<String>,
    #[serde(default)]
    pub audio: Vec<String>,
    #[serde(default)]
    pub streamable: bool,
    #[serde(default)]
    pub properties: BTreeMap<String, PropValue>,
    #[serde(default)]
    pub comment: Option<String>,
}

impl ContainerEntry {
    pub fn carries_video(&self, codec: &str) -> bool {
        self.video.iter().any(|c| c == codec)
    }
    pub fn carries_audio(&self, codec: &str) -> bool {
        self.audio.iter().any(|c| c == codec)
    }
}

impl VideoEntry {
    pub fn id(&self) -> String {
        derived_id(&self.id, &self.codec, &self.accel, &self.encoder, &self.decoder)
    }
    pub fn element(&self, role: Role) -> Option<&str> {
        match role {
            Role::Encode => self.encoder.as_deref(),
            Role::Decode => self.decoder.as_deref(),
        }
    }
    /// Elements that must be in the registry for this entry to fill this role.
    ///
    /// The role's own element is always required. Anything in `requires` is
    /// required too, except the other role's element: writing
    /// `requires = ["nvh264enc"]` on an entry that also names `nvh264dec` must
    /// not take the decoder away from a machine whose NVENC is unusable.
    pub fn needs(&self, role: Role) -> Vec<String> {
        needs(self.element(role), self.other(role), &self.requires)
    }
    fn other(&self, role: Role) -> Option<&str> {
        match role {
            Role::Encode => self.decoder.as_deref(),
            Role::Decode => self.encoder.as_deref(),
        }
    }
}

impl AudioEntry {
    pub fn id(&self) -> String {
        derived_id(&self.id, &self.codec, &self.accel, &self.encoder, &self.decoder)
    }
    pub fn element(&self, role: Role) -> Option<&str> {
        match role {
            Role::Encode => self.encoder.as_deref(),
            Role::Decode => self.decoder.as_deref(),
        }
    }
    pub fn needs(&self, role: Role) -> Vec<String> {
        let other = match role {
            Role::Encode => self.decoder.as_deref(),
            Role::Decode => self.encoder.as_deref(),
        };
        needs(self.element(role), other, &self.requires)
    }
}

impl GraphicsEntry {
    pub fn id(&self) -> String {
        self.id.clone().unwrap_or_else(|| self.accel.clone())
    }
    pub fn needs(&self) -> Vec<String> {
        let mut v: Vec<String> = self.requires.clone();
        for e in [&self.compositor, &self.convert] {
            if !e.is_empty() && !v.iter().any(|r| r == e) {
                v.push(e.clone());
            }
        }
        v
    }
}

fn needs(own: Option<&str>, other: Option<&str>, requires: &[String]) -> Vec<String> {
    let mut v = Vec::with_capacity(requires.len() + 1);
    if let Some(e) = own {
        v.push(e.to_string());
    }
    for r in requires {
        if Some(r.as_str()) == other || v.iter().any(|x| x == r) {
            continue;
        }
        v.push(r.clone());
    }
    v
}

fn derived_id(
    explicit: &Option<String>,
    codec: &str,
    accel: &str,
    encoder: &Option<String>,
    decoder: &Option<String>,
) -> String {
    if let Some(id) = explicit {
        return id.clone();
    }
    let element = encoder.as_deref().or(decoder.as_deref()).unwrap_or("none");
    format!("{codec}-{accel}-{element}")
}
