//! What `program.transitions` answers with.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `program.transitions`: every transition a take may name on this core.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct TransitionCatalogue {
    pub transitions: Vec<TransitionEntry>,
    /// The four easings every transition takes as `params.easing`.
    pub easings: Vec<String>,
    /// The directions `wipe`, `slide` and `push` take.
    pub directions: Vec<String>,
    /// What an item's `enter` and `exit` may be.
    pub item_transitions: Vec<String>,
    /// The edges an item transition takes.
    pub edges: Vec<String>,
    /// The longest a transition may run, in milliseconds.
    pub max_duration_ms: u64,
    /// What a name on its own runs for, in milliseconds.
    pub default_duration_ms: u64,
}

/// One name a take accepts.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TransitionEntry {
    pub name: String,
    /// `built-in`, `collection` (a named transition the scene collection
    /// stores) or `plugin`.
    pub origin: String,
    /// The type underneath a collection's name, which is the name itself for
    /// the other two.
    #[serde(rename = "type")]
    pub type_id: String,
    /// The params it reads, for a built in one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<String>,
    /// The duration a collection stores with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
}

