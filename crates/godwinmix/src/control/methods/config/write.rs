//! `config.set` and `config.reset`: check, write, then apply what can be live.

use super::read::{config_file, held, FileState};
use crate::control::call::Call;
use crate::control::methods::body;
use godwinmix_core::config::keys::{self, Applies};
use godwinmix_core::config::settable::{self, Change, Refused};
use godwinmix_core::config::Config;
use godwinmix_core::mixer::Command;
use godwinmix_protocol::error::{ErrorCode, RpcError};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConfigSetRequest {
    /// Dotted key to new value: `{"program.video_bitrate_kbps": 4500}`. Null
    /// puts a key back to its default. For a secret, the sentinel
    /// `"__secret__"` means leave it as it is, and an empty string clears it.
    pub values: Map<String, Value>,
    /// Check everything and write nothing.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct ConfigResetRequest {
    /// Dotted keys to take out of the config file, so their defaults apply.
    pub keys: Vec<String>,
    #[serde(default)]
    pub dry_run: bool,
}

/// One key this call changed, and when the change takes effect.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigChanged {
    pub key: String,
    pub applies: Applies,
    /// Said when something outside the file wins over it, such as `--bind`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// What `config.set` and `config.reset` answer with.
#[derive(Debug, Serialize, schemars::JsonSchema)]
pub struct ConfigSetResult {
    /// True when nothing was written because `dry_run` was set.
    pub dry_run: bool,
    /// The config file written to.
    pub path: String,
    /// Every key this call changed, each with its `applies`.
    pub changed: Vec<ConfigChanged>,
    /// Secrets sent back as the sentinel, so left as they were.
    pub unchanged: Vec<String>,
    /// Keys from this call in force now.
    pub applied: Vec<String>,
    /// Keys from this call every source added or rebuilt from now on uses.
    pub next_source: Vec<String>,
    /// Every key, from this call or an earlier one, whose new value waits for
    /// a restart. Empty is the good case.
    pub needs_restart: Vec<String>,
}

pub(super) async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ConfigSetRequest = call.params(&params)?;
    if req.values.is_empty() {
        return Err(RpcError::invalid_params(
            "config.set was sent no values. Send `values` with at least one dotted key, \
             for example {\"values\": {\"program.video_bitrate_kbps\": 4500}}.",
        ));
    }
    let mut changes = Vec::new();
    let mut unchanged = Vec::new();
    for (key, value) in &req.values {
        let row = settable::settable(key).map_err(refusal)?;
        if row.secret && value.as_str() == Some(godwinmix_core::secrets::SENTINEL) {
            unchanged.push(key.clone());
            continue;
        }
        changes.push(settable::check(key, value).map_err(refusal)?);
    }
    commit(&call, changes, unchanged, req.dry_run || call.dry_run).await
}

pub(super) async fn reset(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: ConfigResetRequest = call.params(&params)?;
    if req.keys.is_empty() {
        return Err(RpcError::invalid_params(
            "config.reset was sent no keys. Send `keys` with the dotted keys to put back, \
             for example {\"keys\": [\"canvas.width\"]}.",
        ));
    }
    let mut changes = Vec::new();
    for key in &req.keys {
        settable::settable(key).map_err(refusal)?;
        changes.push((key.clone(), None));
    }
    commit(&call, changes, Vec::new(), req.dry_run || call.dry_run).await
}

/// Write the changes, apply the live ones, and say what waits.
async fn commit(call: &Call, changes: Vec<Change>, unchanged: Vec<String>, dry_run: bool) -> Result<Value, RpcError> {
    let path = config_file(call)?;
    let written = {
        let path = path.clone();
        let changes = changes.clone();
        tokio::task::spawn_blocking(move || settable::write(&path, &changes, dry_run))
            .await
            .map_err(|e| RpcError::internal(format!("the config write stopped: {e}")))?
    };
    let new = written
        .map_err(|e| RpcError::internal(format!("writing {}: {e:#}", path.display())))?
        .map_err(refusal)?;
    let rows: Vec<&keys::Key> = changes.iter().filter_map(|(k, _)| keys::find(k)).collect();
    if !dry_run && rows.iter().any(|k| k.applies != Applies::Restart) {
        apply_live(call, &new).await?;
    }
    let state = FileState::read(&path)?;
    let overrides = held().read().as_ref().map(|h| h.overrides.clone()).unwrap_or_default();
    let named = |a: Applies| rows.iter().filter(|k| k.applies == a).map(|k| k.key.to_string()).collect();
    body(ConfigSetResult {
        dry_run,
        path: path.display().to_string(),
        changed: rows
            .iter()
            .map(|k| ConfigChanged {
                key: k.key.to_string(),
                applies: k.applies,
                note: overrides.get(k.key).map(|by| format!("{by} wins over the file for this key")),
            })
            .collect(),
        unchanged,
        applied: named(Applies::Live),
        next_source: named(Applies::NextSource),
        needs_restart: if dry_run { named(Applies::Restart) } else { state.pending() },
    })
}

/// Hand the running core what it can take now: the mixer loop's own copy and
/// the safety guard. Nothing here rebuilds a pipeline.
async fn apply_live(call: &Call, new: &Config) -> Result<(), RpcError> {
    let cfg = Box::new(new.clone());
    call.app
        .mixer
        .request(|ack| Command::Reconfigure(cfg, Some(ack)))
        .await
        .map_err(|e| call.mixer_error(e))?;
    call.app.safety.set_config(new.safety.clone());
    if let Some(held) = held().write().as_mut() {
        keys::take_live(&mut held.running, new);
    }
    Ok(())
}

/// A core refusal as the error a client reads.
pub(super) fn refusal(r: Refused) -> RpcError {
    let code = if r.data.contains_key("valid") { ErrorCode::NotFound } else { ErrorCode::InvalidParams };
    RpcError::new(code, r.message).with_data(r.data)
}
