//! What the script methods answer, and the event a saved script sends.

use super::Row;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One script, as `prompter.script.get` and `prompter.script.save` answer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Script {
    /// A slug: `evening-news`.
    pub id: String,
    pub title: String,
    /// Plain text. A line starting with `#` is a marker, `*word*` is drawn in
    /// italics and `**word**` in bold.
    pub text: String,
    /// When it was last saved, in milliseconds since the Unix epoch.
    pub updated_ms: u64,
}

/// One script in `prompter.script.list`, without its text.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptSummary {
    pub id: String,
    pub title: String,
    /// Lines as written, blank ones included.
    pub lines: u32,
    /// The names of its markers, in order.
    pub markers: Vec<String>,
    pub bytes: u32,
    pub updated_ms: u64,
}

/// `prompter.script.list`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptList {
    /// Every script, by id.
    pub scripts: Vec<ScriptSummary>,
    /// The one on the prompter now.
    pub on: Option<String>,
}

/// `prompter.rows`: the script on the prompter, broken into rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RowsAnswer {
    pub script: Option<String>,
    pub updated_ms: u64,
    /// The width the rows were made at.
    pub line_chars: u32,
    pub rows: Vec<Row>,
}

/// `event/prompter.script.changed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ScriptChanged {
    pub id: String,
    /// True when the script was removed.
    pub removed: bool,
    pub updated_ms: u64,
}
