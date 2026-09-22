//! One of the things a person does after applying a preset.
//!
//! A step used to be a sentence, and every sentence the six presets carried
//! told the person to open a file or a terminal. A step now says what it
//! does and what it does it to, so a page can draw a button for it and tick
//! it off when the mixer's own state says it is done. The sentence stays:
//! the CLI prints it, and a page that does not know `does` shows it as prose.
//!
//! In a manifest a step is either a plain string, which is what a third
//! party preset written before this had, or a table:
//!
//! ```toml
//! [[provides.preset.steps]]
//! text = "Add the YouTube stream key."
//! does = "add-key"
//! target = "youtube"
//! ```

use serde::{Deserialize, Serialize};

/// What a step can do, and what its `target` names for each.
///
/// * `add-key`: an output id. Opens the output's key form; done once the
///   output has a key.
/// * `add-source`: a picker category (`cameras`, `screens`, `streams`,
///   `files`, `web`). Opens the picker there; done once a source is added.
/// * `install-plugin`: a plugin name. Installs it; done once it is loaded.
/// * `open-panel`: a panel id such as `core/alerts`. Brings that panel into
///   view; done once it has been looked at.
/// * `take`: a source or scene id. Puts it on air; done while it is on air.
pub const ACTIONS: &[&str] = &["add-key", "add-source", "install-plugin", "open-panel", "take"];

/// One step: the sentence, and optionally what a page can do about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(from = "Written")]
pub struct Step {
    /// What to do, in a sentence that names no file, command or address.
    pub text: String,
    /// One of [`ACTIONS`], or absent for a step only a person can do.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub does: Option<String>,
    /// The id `does` acts on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// The two ways a manifest may write a step.
#[derive(Deserialize)]
#[serde(untagged)]
enum Written {
    Prose(String),
    Table {
        text: String,
        #[serde(default)]
        does: Option<String>,
        #[serde(default)]
        target: Option<String>,
    },
}

impl From<Written> for Step {
    fn from(w: Written) -> Self {
        match w {
            Written::Prose(text) => Step { text, does: None, target: None },
            Written::Table { text, does, target } => Step { text, does, target },
        }
    }
}

impl Step {
    /// What is wrong with this step, if anything: an action nobody knows, or
    /// an action with nothing to act on.
    pub fn problem(&self) -> Option<String> {
        let does = self.does.as_deref()?;
        if !ACTIONS.contains(&does) {
            return Some(format!(
                "the step {:?} does {does:?}, which is not one of {}",
                self.text,
                ACTIONS.join(", ")
            ));
        }
        match self.target.as_deref() {
            Some(t) if !t.trim().is_empty() => None,
            _ => Some(format!("the step {:?} does {does:?} but names no target", self.text)),
        }
    }
}

/// The sentences alone, for the CLI and for a page that predates `does`.
pub fn texts(steps: &[Step]) -> Vec<String> {
    steps.iter().map(|s| s.text.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Deserialize)]
    struct Holder {
        steps: Vec<Step>,
    }

    #[test]
    fn a_step_reads_as_a_string_or_as_a_table() {
        let got: Holder = toml::from_str(
            "steps = [\"Press a tile.\", { text = \"Add the key.\", does = \"add-key\", target = \"youtube\" }]",
        )
        .unwrap();
        assert_eq!(got.steps[0], Step { text: "Press a tile.".into(), does: None, target: None });
        assert_eq!(got.steps[1].does.as_deref(), Some("add-key"));
        assert_eq!(got.steps[1].target.as_deref(), Some("youtube"));
    }

    #[test]
    fn a_step_goes_out_with_only_the_fields_it_has() {
        let prose = serde_json::to_value(Step { text: "Go.".into(), does: None, target: None }).unwrap();
        assert_eq!(prose, serde_json::json!({ "text": "Go." }));
    }

    #[test]
    fn an_unknown_action_or_a_missing_target_is_a_problem() {
        let odd = Step { text: "x".into(), does: Some("edit-toml".into()), target: Some("a".into()) };
        assert!(odd.problem().unwrap().contains("add-key"));
        let bare = Step { text: "x".into(), does: Some("add-key".into()), target: None };
        assert!(bare.problem().unwrap().contains("no target"));
        let prose = Step { text: "x".into(), does: None, target: None };
        assert!(prose.problem().is_none());
    }
}
