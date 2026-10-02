//! Pieces of the mixer that are set up the first time somebody needs them.
//!
//! A web page needs the browser renderer, a camera needs the camera plugin,
//! a channel needs the ingest plugin. A packaged mixer carries all of them;
//! a mixer run from a source checkout builds or installs each one the first
//! time it is asked for. This is the record of where each one stands, which
//! `setup.list` answers with and `event/setup.changed` carries.
//!
//! `message` is for a person: what does not work yet and what is happening
//! about it, with no program names in it. `detail` is for whoever reads the
//! log: paths, commands, exit codes.

use crate::action::ErrorAction;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One piece that lives outside the mixer's own program.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetupStatus {
    /// `web` for the browser renderer, otherwise the plugin's name.
    pub piece: String,
    /// What it gives a person, in their words: "Web pages", "Cameras".
    pub title: String,
    pub state: SetupState,
    /// One or two plain sentences: what is happening and what comes next.
    pub message: String,
    /// 0 to 1 while a download says how far it has got.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// The button that moves it on, when there is one: try again, or a
    /// command to copy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ErrorAction>,
    /// For a developer: where it looked, what it ran, where the log is.
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub detail: Value,
}

/// Where a piece stands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SetupState {
    /// There and usable.
    Ready,
    /// Not there, and this mixer can set it up when asked.
    #[default]
    Missing,
    /// Being set up now.
    Running,
    /// The last attempt did not finish. `action` says what to press.
    Failed,
    /// Not there, and this mixer cannot set it up by itself.
    Unavailable,
}

/// `setup.start` and `setup.get`: one piece by name.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SetupRequest {
    /// `web`, or a first party plugin's name such as `camera`.
    pub piece: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_flat_and_leaves_out_what_it_does_not_have() {
        let s = SetupStatus {
            piece: "web".into(),
            title: "Web pages".into(),
            state: SetupState::Running,
            message: "Setting up web pages.".into(),
            ..Default::default()
        };
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["state"], "running");
        assert!(v.get("progress").is_none() && v.get("detail").is_none());
    }
}
