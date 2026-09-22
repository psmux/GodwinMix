//! `config.get` and `config.schema`, and what the running core started with.

use crate::control::call::Call;
use crate::control::methods::body;
use godwinmix_core::config::keys::{Applies, KEYS};
use godwinmix_core::config::schema::{self, lookup, to_json};
use godwinmix_core::config::{settable, Config};
use godwinmix_protocol::error::RpcError;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// What the running core holds: the config it started with, with every live
/// change since copied in, and which keys something outside the file decides.
///
/// A process global, as `presets::Runtime` is, so the config methods cost the
/// shared control plane one call at startup and no new field.
pub(super) struct Held {
    pub running: Config,
    /// `control.bind` when `--bind` was given, and the like: key to reason.
    pub overrides: BTreeMap<String, String>,
}

static HELD: OnceLock<RwLock<Option<Held>>> = OnceLock::new();

pub(super) fn held() -> &'static RwLock<Option<Held>> {
    HELD.get_or_init(|| RwLock::new(None))
}

/// Called once at startup with the config the core runs on, and the keys
/// something other than the file decides (`("control.bind", "--bind")`).
pub fn configure(cfg: &Config, overrides: &[(&str, &str)]) {
    let overrides = overrides.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    *held().write() = Some(Held { running: cfg.clone(), overrides });
}

#[derive(Debug, Default, Deserialize, schemars::JsonSchema)]
pub struct ConfigGetRequest {
    /// Only these dotted keys. Empty or absent is every key.
    #[serde(default)]
    pub keys: Vec<String>,
}

/// One setting as `config.get` reports it.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigKey {
    /// Dotted, as in `program.video_bitrate_kbps`.
    pub key: String,
    /// What the config file says, or the default when it says nothing. Always
    /// null for a secret.
    pub value: Value,
    pub default: Value,
    /// `file` when the key is written in the config file, `default` when not.
    pub source: String,
    pub applies: Applies,
    pub secret: bool,
    /// For a secret: whether one is set. The value itself is never sent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub set: Option<bool>,
    /// What wins over the file for this key, when something does: `--bind`,
    /// or `GODWINMIX_TOKEN` in the core's environment.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub overridden_by: Option<String>,
    /// True when the file differs from what the running core uses and only a
    /// restart will close the gap.
    pub pending: bool,
}

#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigGetResult {
    /// The config file these values are read from and written to.
    pub path: String,
    pub keys: Vec<ConfigKey>,
    /// Every key whose new value waits for a restart, whichever keys were asked for.
    pub needs_restart: Vec<String>,
}

pub(super) async fn get(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ConfigGetRequest = call.params(&params)?;
    for key in &req.keys {
        settable::settable(key).map_err(super::write::refusal)?;
    }
    let path = config_file(&call)?;
    let now = FileState::read(&path)?;
    let keys = KEYS
        .iter()
        .filter(|k| req.keys.is_empty() || req.keys.iter().any(|w| w == k.key))
        .map(|k| now.describe(k))
        .collect();
    body(ConfigGetResult { path: path.display().to_string(), keys, needs_restart: now.pending() })
}

pub(super) async fn schema(_call: Call, _params: Value) -> Result<Value, RpcError> {
    Ok(schema::schema())
}

/// The config file this core writes to, or the refusal for a core with none.
pub(super) fn config_file(call: &Call) -> Result<PathBuf, RpcError> {
    let path = call.app.config_path.as_ref().clone();
    if path.as_os_str().is_empty() || !path.exists() {
        return Err(RpcError::not_in_state(format!(
            "this core has no config file to read or change{}. Start it with --config \
             pointing at one, and the settings can be changed from here.",
            if path.as_os_str().is_empty() { String::new() } else { format!(" ({} is not there)", path.display()) }
        ))
        .with("path", path.display().to_string()));
    }
    Ok(path)
}

/// The file as it is now, beside what the running core uses.
pub(super) struct FileState {
    table: toml::Table,
    file: Value,
    running: Value,
    defaults: Value,
    overrides: BTreeMap<String, String>,
}

impl FileState {
    pub(super) fn read(path: &std::path::Path) -> Result<Self, RpcError> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| RpcError::internal(format!("reading {}: {e}", path.display())))?;
        let table: toml::Table = toml::from_str(&text).map_err(|e| unreadable(path, &e))?;
        let cfg = Config::from_toml(&text, &path.display().to_string()).map_err(|e| unreadable(path, &e))?;
        let held = held().read();
        let running = held.as_ref().map(|h| to_json(&h.running)).unwrap_or_else(|| to_json(&cfg));
        let mut overrides = held.as_ref().map(|h| h.overrides.clone()).unwrap_or_default();
        if godwinmix_core::config::env_var("TOKEN").is_some_and(|t| !t.trim().is_empty()) {
            overrides.insert("control.token".into(), "GODWINMIX_TOKEN".into());
        }
        Ok(Self { table, file: to_json(&cfg), running, defaults: schema::defaults(), overrides })
    }

    fn describe(&self, k: &godwinmix_core::config::keys::Key) -> ConfigKey {
        let value = lookup(&self.file, k.key).cloned().unwrap_or(Value::Null);
        let written = table_has(&self.table, k.key);
        ConfigKey {
            key: k.key.to_string(),
            value: if k.secret { Value::Null } else { value.clone() },
            default: if k.secret { Value::Null } else { lookup(&self.defaults, k.key).cloned().unwrap_or(Value::Null) },
            source: if written { "file" } else { "default" }.into(),
            applies: k.applies,
            secret: k.secret,
            set: k.secret.then(|| value.as_str().is_some_and(|s| !s.is_empty())),
            overridden_by: self.overrides.get(k.key).cloned(),
            pending: self.is_pending(k),
        }
    }

    fn is_pending(&self, k: &godwinmix_core::config::keys::Key) -> bool {
        k.applies == Applies::Restart && lookup(&self.file, k.key) != lookup(&self.running, k.key)
    }

    /// Every key waiting for a restart.
    pub(super) fn pending(&self) -> Vec<String> {
        KEYS.iter().filter(|k| self.is_pending(k)).map(|k| k.key.to_string()).collect()
    }
}

/// Whether the file writes this dotted key itself.
fn table_has(table: &toml::Table, dotted: &str) -> bool {
    let mut at = table;
    let steps: Vec<&str> = dotted.split('.').collect();
    for (n, step) in steps.iter().enumerate() {
        match at.get(*step) {
            Some(toml::Value::Table(inner)) if n + 1 < steps.len() => at = inner,
            Some(_) => return n + 1 == steps.len(),
            None => return false,
        }
    }
    false
}

fn unreadable(path: &std::path::Path, e: &dyn std::fmt::Display) -> RpcError {
    RpcError::not_in_state(format!(
        "{} does not load as it stands ({e}). Fix the file by hand or put back its .bak, \
         then ask again.",
        path.display()
    ))
    .with("path", path.display().to_string())
}
