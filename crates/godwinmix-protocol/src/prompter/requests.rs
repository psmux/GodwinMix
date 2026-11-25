//! What the `prompter.*` methods take.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `prompter.script.get` and `prompter.script.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptIdRequest {
    pub id: String,
}

/// `prompter.script.save`: make a script, or replace one with the same id.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ScriptSaveRequest {
    /// A slug. Made from the title when left out: "Evening news" is
    /// `evening-news`.
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    /// The words. A line starting with `#` is a marker.
    pub text: String,
}

/// `prompter.load`: put a script on the prompter, paused at the top.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LoadRequest {
    pub id: String,
    /// Start playing at once.
    #[serde(default)]
    pub play: bool,
}

/// `prompter.play`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PlayRequest {
    /// Rows a second to play at. Left out keeps the speed it has.
    #[serde(default)]
    pub speed: Option<f64>,
}

/// `prompter.speed`: one of `speed` or `by`.
///
/// Shaped for something that follows the speaker as well as for a person: a
/// plugin listening to the voice sends `speed` as often as it likes, and the
/// screens follow without a jump, since the position carries on from where it
/// was at the moment of the change.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpeedRequest {
    /// Rows a second, -20 to 20. Negative scrolls back.
    #[serde(default)]
    pub speed: Option<f64>,
    /// Add this to the speed it has: what a key or a Stream Deck button sends.
    #[serde(default)]
    pub by: Option<f64>,
}

/// `prompter.seek`: exactly one of these.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SeekRequest {
    /// A row, from 0. 2.5 is half way through the third row.
    #[serde(default)]
    pub row: Option<f64>,
    /// A fraction of the way through: 0 is the top, 1 the end.
    #[serde(default)]
    pub fraction: Option<f64>,
    /// A marker by name, or by its number counting from 1 as a string ("2").
    #[serde(default)]
    pub marker: Option<String>,
    /// Move this many markers on (1 is the next, -1 the one before). From
    /// inside a section, -1 goes to the start of that section.
    #[serde(default)]
    pub step: Option<i32>,
    /// Move this many rows on, or back when negative.
    #[serde(default)]
    pub by: Option<f64>,
}

/// `prompter.set`: change how the text looks. Only what is named changes.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookRequest {
    /// Characters a row holds, 12 to 80. Fewer is bigger.
    #[serde(default)]
    pub line_chars: Option<u32>,
    /// Percent of the width left on each side, 0 to 30.
    #[serde(default)]
    pub margin: Option<f64>,
    #[serde(default)]
    pub mirror: Option<bool>,
    /// Where the cue line sits, 0.1 to 0.9 of the height from the top.
    #[serde(default)]
    pub cue: Option<f64>,
}
