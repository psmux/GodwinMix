//! The params and answers of the wave 4 show methods: `show.set`,
//! `show.output.*` and `show.stats`.

use super::props::InputSpec;
use super::stats::ShowStats;
use crate::rendition::RenditionChoice;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

/// An output as it is given to a show without compositing: an address, or
/// a platform and a key.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputSpec {
    /// A slug, unique within the show. Made from the label when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    /// youtube, facebook, twitch, custom or srt. Left out: custom, which
    /// takes a whole address in `uri`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    /// The whole address: `srt://10.0.0.9:9000`, `rtmp://host/app/key`,
    /// `udp://239.2.2.2:5000`. For a platform, its ingest server when it is
    /// not the platform's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// A platform's stream key. Write only: no method reads it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// On by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Left out: a copy of the input's own bytes, repackaged. Otherwise a
    /// rendition request or `{"preset": "youtube-720p30"}`, planned and
    /// admitted by the governor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendition: Option<RenditionChoice>,
}

/// `show.output.add`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputAddRequest {
    /// The show.
    pub show: String,
    #[serde(flatten)]
    pub output: ShowOutputSpec,
}

/// `show.output.set`. Names only what moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputSetRequest {
    pub show: String,
    /// The output's id.
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// A new key. Left out keeps the one it has.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Left out keeps what it has; `null` or `{"preset": "copy"}` goes back
    /// to a copy.
    #[serde(default, deserialize_with = "present", skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<RenditionChoice>")]
    pub rendition: Option<Option<RenditionChoice>>,
}

fn present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Option<RenditionChoice>>, D::Error> {
    Option::<RenditionChoice>::deserialize(d).map(Some)
}

/// `show.output.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowOutputRemoveRequest {
    pub show: String,
    pub output: String,
}

/// `show.set`. Names only what moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowSetRequest {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// true starts a show process whose one source is the input and moves
    /// the outputs to it; false goes back to a show without compositing,
    /// when it has one source and no scenes in use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
}

/// What a switch of compositing did, in `show.set`'s answer.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SwitchReport {
    /// What the show does now.
    pub compositing: bool,
    /// The outputs that moved.
    pub outputs: Vec<String>,
    /// From the moment the outputs stopped where they were to the moment
    /// every one of them was live again where they went. None when they
    /// were not all live within the wait, or there were none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap_ms: Option<u64>,
    /// What a person should know: an output that was still connecting when
    /// the answer was sent, and so on.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
}

/// `show.set`'s answer: the show, and what a switch of compositing did.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowSetResult {
    #[serde(flatten)]
    pub show: super::Show,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<SwitchReport>,
}

/// `show.stats`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowStatsRequest {
    /// Left out: every show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ids: Option<Vec<String>>,
    /// Of `health`, `input` and `outputs`. Left out: all three.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<Vec<String>>,
}

/// `show.stats`'s answer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowStatsList {
    pub shows: Vec<ShowStats>,
}
