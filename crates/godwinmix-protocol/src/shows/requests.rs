//! The params and answers of the wave 4 show methods: `show.set`,
//! `show.add_many`, `show.remove_many`, `show.output.*` and `show.stats`.

use super::props::{InputSpec, ShowStats};
use super::ShowFrom;
use crate::rendition::{Cost, RenditionChoice};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

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

/// One show of `show.add_many`: what `show.add` takes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowAdd {
    pub name: String,
    /// Left out: true, a show with scenes and a programme, as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compositing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<InputSpec>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outputs: Vec<ShowOutputSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<ShowFrom>,
}

/// `show.add_many`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowAddManyRequest {
    pub shows: Vec<ShowAdd>,
    /// Left out: true. Says what would be made and what it would cost,
    /// and makes nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dry_run: Option<bool>,
}

/// A show of `show.add_many` that was not made, and why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowRefused {
    /// Its place in `shows`, from 0.
    pub index: usize,
    pub name: String,
    pub why: String,
    #[serde(default)]
    pub data: Value,
}

/// What a batch costs, priced by the governor without taking anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BulkPlan {
    /// Every rendition of the shows that fit, summed. Copies cost nothing.
    pub cost: Cost,
    /// What the machine has free now.
    pub have: Cost,
    /// Whether every show of the batch fits.
    pub fits: bool,
    /// The input every rendition was priced against, because an input's
    /// shape is known only once it arrives.
    pub assumed_input: String,
}

/// `show.add_many`'s answer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowAddManyResult {
    /// The ids made, or that would be made on a dry run.
    pub added: Vec<String>,
    pub refused: Vec<ShowRefused>,
    pub plan: BulkPlan,
    pub dry_run: bool,
}

/// `show.remove_many`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowRemoveManyRequest {
    pub ids: Vec<String>,
}

/// `show.remove_many`'s answer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct ShowRemoveManyResult {
    pub removed: Vec<String>,
    /// Ids that were not removed, each with why.
    pub refused: Vec<ShowRefused>,
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

/// `event/show.health`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ShowHealthEvent {
    pub id: String,
    pub health: super::props::Health,
}
