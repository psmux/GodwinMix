//! The bodies of `source.on_loss.set` and the two backup actions.

use super::{OnLoss, ReturnMode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `source.on_loss.set`: only what is named changes. A source with no rule
/// yet needs `backup`; every other field has a default.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetOnLossRequest {
    /// The source that may go down.
    pub id: String,
    /// The source to show instead.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// Seconds to hold the last frame before switching. 3 by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_secs: Option<f64>,
    /// Any transition a take can name, or `cut`. `fade` by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    /// How long the transition runs. 0 is a cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    /// `auto` or `manual`.
    #[serde(default, rename = "return", skip_serializing_if = "Option::is_none")]
    pub returns: Option<ReturnMode>,
    /// With `return = "auto"`, how long the source stays live before it goes
    /// back. 5 by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_after_secs: Option<f64>,
}

impl SetOnLossRequest {
    /// The rule this asks for, laid over the one the source has now.
    pub fn over(&self, current: Option<&OnLoss>) -> Result<OnLoss, String> {
        let mut rule = match (current, &self.backup) {
            (_, Some(backup)) => {
                let mut rule = current.cloned().unwrap_or_else(|| OnLoss::to(backup.trim()));
                rule.backup = backup.trim().to_string();
                rule
            }
            (Some(current), None) => current.clone(),
            (None, None) => {
                return Err(format!(
                    "{} has no backup yet, so say which source to show when it goes down: \
                     {{\"id\": \"{}\", \"backup\": \"cam-wide\"}}.",
                    self.id, self.id
                ))
            }
        };
        if let Some(secs) = self.after_secs {
            rule.after_secs = secs;
        }
        if let Some(name) = &self.transition {
            rule.transition = name.trim().to_string();
        }
        if let Some(ms) = self.duration_ms {
            rule.duration_ms = Some(ms);
        }
        if let Some(mode) = self.returns {
            rule.returns = mode;
        }
        if let Some(secs) = self.return_after_secs {
            rule.return_after_secs = secs;
        }
        Ok(rule)
    }
}

/// `source.backup.return` and `source.backup.stay`: the source by id.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct BackupRequest {
    /// The source that went down, not its backup.
    pub id: String,
}
