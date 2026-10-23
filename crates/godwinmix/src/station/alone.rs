//! What the station answers for a show that is not running.
//!
//! The page reaches a station through a show, the first unless it names
//! another, and a headend starts no show that composites: `core.info` went
//! to a show that was not there, answered 409, and the page said
//! "Disconnected" over a station running fifty channels. Starting a mixer
//! for the page to talk to would cost a process and a compositor nobody
//! asked for. So while the show a connection names has no process, the
//! station answers the calls a page stands up with: who it is, the
//! subscription, and the settings, whose file is the station's own. Every
//! other call of a show's still says the show is not running.

use super::methods;
use super::state::Station;
use godwinmix_core::config::Config;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::scope::Token;
use godwinmix_protocol::shows::ShowState;
use godwinmix_protocol::types::{CanvasInfo, CoreInfo, Limits};
use godwinmix_protocol::SubscribeResult;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// The feature `core.info` carries when the station answered it, so a page
/// knows no mixer stands behind this connection.
pub const FEATURE: &str = "station";

/// What the station's own `core.info` says, read from its config at start.
pub struct Own {
    pub config: PathBuf,
    pub canvas: CanvasInfo,
    pub limits: Limits,
    pub rehearsal: bool,
}

impl Own {
    pub fn new(config: PathBuf, cfg: &Config, rehearsal: bool) -> Own {
        let limits = Limits {
            max_upload_bytes: cfg.media.max_upload_bytes,
            max_gain: crate::control::MAX_GAIN,
            max_call_secs: godwinmix_protocol::MAX_CALL_SECS,
            max_idempotency_key_bytes: 255,
            event_queue: 256,
        };
        let canvas = CanvasInfo { width: cfg.canvas.width, height: cfg.canvas.height, fps: cfg.canvas.fps };
        Own { config, canvas, limits, rehearsal }
    }
}

/// The calls the station answers in a show's place.
pub fn answers(method: &str) -> bool {
    matches!(method, "core.info" | "core.subscribe" | "config.get" | "config.schema" | "config.set" | "config.reset")
}

/// Whether `show` has no process to answer: stopped, failed, or a show
/// without compositing, which never has one. A show that is starting is
/// waited for, as before.
pub fn stands_in(st: &Station, show: &str) -> bool {
    st.alone.get().is_some() && (st.is_direct(show) || matches!(st.state_of(show), Some(ShowState::Stopped | ShowState::Failed)))
}

/// Answer `method` for a connection of `token`, named `client`, whose
/// subscription is `events`.
pub async fn call(st: &Arc<Station>, token: &Token, client: &str, method: &str, params: Value, events: &[String]) -> Result<Value, RpcError> {
    let Some(own) = st.alone.get() else { return Err(RpcError::internal("the station has not finished starting")) };
    let Some(def) = methods::registry().get(method) else { return Err(RpcError::internal(format!("{method} is not in the method table"))) };
    if !token.has(def.scope) {
        return Err(RpcError::scope(method, def.scope.as_str(), &token.scope_names()));
    }
    use crate::control::methods::config as cfg;
    match method {
        "core.info" => Ok(serde_json::to_value(info(st, own, token, client)).unwrap_or_default()),
        "core.subscribe" => {
            let result = SubscribeResult { seq: 0, events: events.to_vec(), ignored_ext: Vec::new(), client_id: Some(client.to_string()) };
            Ok(serde_json::to_value(result).unwrap_or_default())
        }
        "config.schema" => Ok(godwinmix_core::config::schema::schema()),
        "config.get" => cfg::get_in(&cfg::file_at(own.config.clone())?, read(method, params)?),
        "config.set" => cfg::set_in(&cfg::file_at(own.config.clone())?, read(method, params)?, false, None).await,
        "config.reset" => cfg::reset_in(&cfg::file_at(own.config.clone())?, read(method, params)?, false, None).await,
        other => Err(RpcError::internal(format!("the station does not stand in for {other}"))),
    }
}

fn read<T: DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    let params = if params.is_null() { Value::Object(Default::default()) } else { params };
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method} could not read its params: {e}")))
}

fn info(st: &Station, own: &Own, token: &Token, client: &str) -> CoreInfo {
    use crate::control::methods::{lifecycle, presets};
    let mut features = vec!["api-v1".to_string(), "rpc".into(), FEATURE.into()];
    if st.headend.get().and_then(|h| h.upgrade()).is_some() {
        features.push("headend".into());
    }
    if !st.tokens.is_open() {
        features.push("tokens".into());
    }
    features.sort();
    CoreInfo {
        core: "godwinmix".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        api_level: godwinmix_protocol::API_LEVEL,
        api_compatible: godwinmix_protocol::API_COMPATIBLE,
        features,
        limits: own.limits.clone(),
        canvas: own.canvas,
        token: Some(token.info()),
        client_id: Some(client.to_string()),
        rehearsal: own.rehearsal,
        ui: presets::ui_defaults(),
        supervised: lifecycle::supervised(),
        restart: lifecycle::restart_info(),
        tls: crate::tls::info(),
        executable: std::env::current_exe().ok().map(|p| p.display().to_string()),
        started_ms: Some(lifecycle::started_ms()),
    }
}

/// The connection's name as a show would give it: `<token id>.<client_id>`,
/// or the token id alone when the query names none.
pub fn client_name(token: &Token, query: Option<&str>) -> String {
    let named = query.and_then(|q| q.split('&').find_map(|p| p.strip_prefix("client_id=").filter(|v| !v.is_empty())));
    match named {
        Some(name) => format!("{}.{name}", token.id),
        None => token.id.clone(),
    }
}
