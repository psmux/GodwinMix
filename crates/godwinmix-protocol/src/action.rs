//! What a person can press to get past a refusal.
//!
//! A refusal's message says the state and the next step in English. When that
//! next step is something a client can do on the person's behalf (change one
//! setting, install a plugin, restart the mixer), the error also carries it as
//! `data.action`, so a page puts a button under the message instead of parsing
//! the prose. The same object rides on an `alert` event.
//!
//! ```json
//! { "kind": "set-config", "label": "Allow command sources",
//!   "key": "security.allow_exec_sources", "value": true, "applies": "live" }
//! ```
//!
//! A client that does not know a `kind` ignores the action and shows the
//! message, which still reads on its own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One thing a client can offer as a button. `label` is the button's text,
/// `kind` says what pressing it does, and the other fields are the ones that
/// kind uses. Flat rather than an enum with data, so every generated client
/// reads every field.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ErrorAction {
    pub kind: ActionKind,
    /// Short, in the imperative, for a person: "Turn the multiview on".
    pub label: String,
    /// `set-config`: the dotted key. `open`: the setting to show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    /// `set-config`: the value to send.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// `set-config`: what `config.get` says about the key: `live`,
    /// `next_source` or `restart`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applies: Option<String>,
    /// `install-plugin` and `enable-plugin`: the plugin.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `open`: a panel by id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub panel: Option<String>,
    /// `open`: a dialog, such as `settings`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dialog: Option<String>,
    /// `retry`: how long to wait first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_ms: Option<u64>,
}

/// What pressing the button does.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ActionKind {
    /// Call `config.set {values: {key: value}}`.
    SetConfig,
    /// Call `plugin.add {source: name}`.
    InstallPlugin,
    /// Call `plugin.enable {name}`.
    EnablePlugin,
    /// Open a part of the client.
    #[default]
    Open,
    /// Send the same call again after `after_ms`.
    Retry,
    /// Call `core.restart`, when `core.info` says a restart is possible.
    Restart,
}

impl ErrorAction {
    pub fn new(label: impl Into<String>, kind: ActionKind) -> Self {
        Self { kind, label: label.into(), ..Default::default() }
    }

    pub fn set_config(label: &str, key: &str, value: impl Into<Value>, applies: &str) -> Self {
        Self {
            key: Some(key.into()),
            value: Some(value.into()),
            applies: Some(applies.into()),
            ..Self::new(label, ActionKind::SetConfig)
        }
    }

    pub fn install_plugin(name: &str) -> Self {
        Self { name: Some(name.into()), ..Self::new(format!("Install {name}"), ActionKind::InstallPlugin) }
    }

    pub fn enable_plugin(name: &str) -> Self {
        Self { name: Some(name.into()), ..Self::new(format!("Turn {name} on"), ActionKind::EnablePlugin) }
    }

    pub fn open_setting(label: &str, key: &str) -> Self {
        Self {
            dialog: Some("settings".into()),
            key: Some(key.into()),
            ..Self::new(label, ActionKind::Open)
        }
    }

    pub fn open_panel(label: &str, panel: &str) -> Self {
        Self { panel: Some(panel.into()), ..Self::new(label, ActionKind::Open) }
    }

    pub fn restart() -> Self {
        Self::new("Restart the mixer", ActionKind::Restart)
    }

    pub fn to_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }

    /// The first action anywhere in an error's chain, for code that holds a
    /// boxed error (an `anyhow::Error` derefs to one) and turns it into an
    /// `RpcError` at the edge.
    pub fn find(err: &(dyn std::error::Error + 'static)) -> Option<ErrorAction> {
        let mut at: Option<&(dyn std::error::Error + 'static)> = Some(err);
        while let Some(e) = at {
            if let Some(found) = e.downcast_ref::<Actionable>() {
                return Some(found.action.clone());
            }
            at = e.source();
        }
        None
    }

    /// Every kind, with the fields it carries, for `protocol.json`.
    pub fn table() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            ("set-config", "key, value, applies", "config.set with that one value; a restart follows when applies is restart"),
            ("install-plugin", "name", "plugin.add with the name as the source"),
            ("enable-plugin", "name", "plugin.enable"),
            ("open", "panel, dialog, key", "show that part of the client"),
            ("retry", "after_ms", "the same call again, after the wait"),
            ("restart", "", "core.restart, when core.info says restart.possible"),
        ]
    }
}

/// A refusal raised deep in the engine that already knows its button. It
/// travels inside an `anyhow::Error` and the control layer lifts the action
/// into `data.action` with `ErrorAction::find`.
#[derive(Debug, Clone)]
pub struct Actionable {
    pub message: String,
    pub action: ErrorAction,
}

impl Actionable {
    pub fn new(message: impl Into<String>, action: ErrorAction) -> Self {
        Self { message: message.into(), action }
    }
}

impl std::fmt::Display for Actionable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Actionable {}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn an_action_is_a_flat_object_with_its_kind() {
        let a = ErrorAction::set_config("Allow command sources", "security.allow_exec_sources", true, "live");
        assert_eq!(
            a.to_value(),
            json!({ "label": "Allow command sources", "kind": "set-config",
                    "key": "security.allow_exec_sources", "value": true, "applies": "live" })
        );
        assert_eq!(ErrorAction::restart().to_value(), json!({ "label": "Restart the mixer", "kind": "restart" }));
        let back: ErrorAction = serde_json::from_value(a.to_value()).unwrap();
        assert_eq!(back, a);
        // Every kind in the table is one serde spells the same way.
        for (kind, _, _) in ErrorAction::table() {
            let parsed: ActionKind = serde_json::from_value(json!(kind)).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), json!(kind));
        }
    }

    #[test]
    fn the_action_is_found_under_context() {
        #[derive(Debug)]
        struct Outer(Actionable);
        impl std::fmt::Display for Outer {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("adding cam1")
            }
        }
        impl std::error::Error for Outer {
            fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
                Some(&self.0)
            }
        }
        let e = Outer(Actionable::new("no", ErrorAction::install_plugin("ndi")));
        assert_eq!(ErrorAction::find(&e), Some(ErrorAction::install_plugin("ndi")));
    }
}
