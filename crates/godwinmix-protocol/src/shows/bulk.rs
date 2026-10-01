//! `show.add_many` and `show.remove_many`: their params and answers.

use super::props::InputSpec;
use super::requests::ShowOutputSpec;
use super::ShowFrom;
use crate::rendition::Cost;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
