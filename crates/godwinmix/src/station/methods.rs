//! What the station answers itself: `show.*`, `channel.*`, the governor,
//! and a channel's rendition plan. Everything else belongs to a show.
//!
//! The scopes are the method table's, the same one every show checks
//! against, so a token reaches exactly what it would in a single process.

use super::state::Station;
use crate::control::call::Call;
use godwinmix_core::render::status;
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::rendition::CalibrateResult;
use godwinmix_protocol::scope::Token;
use serde_json::Value;
use std::sync::{Arc, OnceLock};

/// The method table, built once.
pub fn registry() -> &'static Registry<Call> {
    static REG: OnceLock<Registry<Call>> = OnceLock::new();
    REG.get_or_init(crate::control::methods::registry)
}

/// Is this call the station's to answer?
pub fn answers(method: &str, params: &Value) -> bool {
    method.starts_with("show.")
        || method.starts_with("channel.")
        || method == "governor.status"
        || method == "governor.calibrate"
        || (method == "rendition.plan"
            && params.get("scope").and_then(Value::as_str).is_some_and(|s| s.starts_with("channel:")))
}

/// Answer one of the station's methods for `token`.
pub async fn call(st: &Arc<Station>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    let Some(def) = registry().get(method) else {
        let near: Vec<String> = registry().nearest(method).iter().map(|s| s.to_string()).collect();
        return Err(RpcError::not_found("method", method, &near));
    };
    if !token.has(def.scope) {
        return Err(RpcError::scope(method, def.scope.as_str(), &token.scope_names()));
    }
    match method {
        m if m.starts_with("show.") => super::shows_call::call(st, m, params).await,
        m if m.starts_with("channel.") => super::channel_calls::call(st, token, m, params).await,
        "governor.status" => Ok(serde_json::to_value(status::governor_status(&st.render, &[])).unwrap_or_default()),
        "governor.calibrate" => calibrate(st, &params),
        "rendition.plan" => plan(st, &params),
        other => Err(RpcError::not_found("method", other, &[])),
    }
}

fn calibrate(st: &Station, params: &Value) -> Result<Value, RpcError> {
    let confirm = params.get("confirm").and_then(Value::as_bool).unwrap_or(false);
    if st.render.on_air() && !confirm {
        let on_air: Vec<String> = st.on_air.lock().iter().cloned().collect();
        return Err(RpcError::not_in_state(format!(
            "{} on air, and measuring takes every core for a few seconds, which can cost it frames. \
             Measure once nothing is going out, or send confirm true to measure now anyway.",
            on_air.join(", ")
        ))
        .with("on_air", on_air));
    }
    Ok(serde_json::to_value(CalibrateResult { started: st.render.calibrate_now() }).unwrap_or_default())
}

fn plan(st: &Station, params: &Value) -> Result<Value, RpcError> {
    let scope = params.get("scope").and_then(Value::as_str).unwrap_or_default();
    let id = scope.trim_start_matches("channel:");
    let found = st.channels.get().and_then(|c| c.rendition_plan(id));
    match found {
        Some(view) => Ok(serde_json::to_value(view).unwrap_or_default()),
        None => Err(RpcError::not_found("plan scope", scope, &[])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_station_answers_its_own_methods_and_leaves_the_rest_to_a_show() {
        assert!(answers("show.list", &json!({})));
        assert!(answers("channel.add", &json!({})));
        assert!(answers("governor.status", &json!({})));
        assert!(answers("rendition.plan", &json!({"scope": "channel:church"})));
        assert!(!answers("rendition.plan", &json!({})));
        assert!(!answers("program.take", &json!({})));
        assert!(!answers("core.subscribe", &json!({})));
    }
}
