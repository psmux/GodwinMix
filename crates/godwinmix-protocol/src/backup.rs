//! A backup source that takes over when a source goes down.
//!
//! ```toml
//! [[sources]]
//! id = "cam-main"
//! uri = "srt://..."
//! on_loss = { backup = "cam-wide", after_secs = 3, transition = "fade", duration_ms = 500, return = "auto" }
//! ```
//!
//! The rule lives on the source that may go down. Wherever that source is
//! drawn (the programme, a scene, a picture in picture) its backup is drawn
//! in its place once it has had no picture for `after_secs`, and it comes
//! back when it is steady again, or when the operator says so.
//!
//! This module is the shape of it on the wire and in the config, and the
//! checks that need no mixer: an unknown backup and a loop of backups are
//! refused here, for the config file and for `source.on_loss.set` alike.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod check;
mod request;
#[cfg(test)]
mod tests;

pub use check::{check, known_transition, Problem};
pub use request::{BackupRequest, SetOnLossRequest};

/// How long a source holds its last frame before the backup goes on.
pub const DEFAULT_AFTER_SECS: f64 = 3.0;
/// How long a source that came back has to stay steady before it returns.
pub const DEFAULT_RETURN_AFTER_SECS: f64 = 5.0;
/// The transition both ways when none is named.
pub const DEFAULT_TRANSITION: &str = "fade";
/// The longest wait either way. An hour is longer than any show needs, and a
/// typo of 30000 is caught rather than left to wait for eight hours.
pub const MAX_WAIT_SECS: f64 = 3600.0;
/// The longest transition, the same ten seconds a take is held to.
pub const MAX_TRANSITION_MS: u64 = 10_000;

/// `on_loss` on a source: what to show when it goes down.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OnLoss {
    /// The source shown instead: any source id this mixer has, a camera, a
    /// looping clip, a still or a graphic. When it is down too, its own
    /// `on_loss` backup is used, if it has one.
    pub backup: String,
    /// Seconds to hold the last frame before switching. 0 switches at once.
    #[serde(default = "default_after")]
    pub after_secs: f64,
    /// Any transition a take can name, or `cut`. `dissolve` is read as `fade`.
    #[serde(default = "default_transition")]
    pub transition: String,
    /// How long the transition runs. Absent takes the transition's own
    /// default, 300 ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `auto` goes back once the source is steady again; `manual` stays on
    /// the backup until the operator calls `source.backup.return`.
    #[serde(default, rename = "return")]
    pub returns: ReturnMode,
    /// With `return = "auto"`, how long the source has to stay live before
    /// it goes back, so a feed that flickers does not flap the programme.
    #[serde(default = "default_return_after")]
    pub return_after_secs: f64,
}

fn default_after() -> f64 {
    DEFAULT_AFTER_SECS
}

fn default_transition() -> String {
    DEFAULT_TRANSITION.to_string()
}

fn default_return_after() -> f64 {
    DEFAULT_RETURN_AFTER_SECS
}

impl OnLoss {
    /// A rule with every default, falling back to `backup`.
    pub fn to(backup: impl Into<String>) -> Self {
        OnLoss {
            backup: backup.into(),
            after_secs: DEFAULT_AFTER_SECS,
            transition: default_transition(),
            duration_ms: None,
            returns: ReturnMode::Auto,
            return_after_secs: DEFAULT_RETURN_AFTER_SECS,
        }
    }

    /// The transition's name as a take reads it: trimmed, lowercased, and
    /// `dissolve` spelled the way this mixer spells it.
    pub fn transition_name(&self) -> String {
        match self.transition.trim().to_lowercase().as_str() {
            "" | "dissolve" => "fade".into(),
            other => other.to_string(),
        }
    }
}

/// How the programme goes back to a source that came back.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ReturnMode {
    /// By itself, once the source has stayed live for `return_after_secs`.
    #[default]
    Auto,
    /// Only when the operator says so.
    Manual,
}

/// Where a source with a backup stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BackupPhase {
    /// The source is live and drawn as itself.
    Ready,
    /// It has no picture; its last frame is held until `switch_in_ms` runs out.
    Lost,
    /// Its backup is drawn in its place.
    OnBackup,
    /// It is live again and goes back when `return_in_ms` runs out.
    Returning,
    /// It is live again and the backup stays until the operator returns it.
    MainBack,
}

/// `backup` on a source's status, present when it has `on_loss`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BackupState {
    pub phase: BackupPhase,
    /// The source drawn in this one's place, while one is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub showing: Option<String>,
    /// How long this source has had no picture, while it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lost_ms: Option<u64>,
    /// In `lost`, how long until the backup goes on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch_in_ms: Option<u64>,
    /// In `returning`, how long until the source goes back on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_in_ms: Option<u64>,
    /// Something the operator should know, such as the backup being down
    /// too, in a sentence that says what happens because of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
}
