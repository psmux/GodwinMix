//! What the station answers itself: `show.*`, `channel.*`, `token.*`, the
//! governor, and a channel's rendition plan. Everything else belongs to a show.
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
        || method.starts_with("token.")
        || method == "network.share"
        || method == "governor.status"
        || method == "governor.calibrate"
        || (method == "rendition.plan"
            && params.get("scope").and_then(Value::as_str).is_some_and(|s| s.starts_with("channel:")))
        || (matches!(method, "task.get" | "task.cancel") && task_id(params).is_some_and(super::switch::task::is_station_task))
}

/// The task a `task.get` names: `task_id`, or `id` from a REST path.
fn task_id(params: &Value) -> Option<&str> {
    params.get("task_id").or_else(|| params.get("id")).and_then(Value::as_str)
}

/// Answer one of the station's methods for `token`.
pub async fn call(st: &Arc<Station>, token: &Token, method: &str, params: Value) -> Result<Value, RpcError> {
    let Some(def) = registry().get(method) else { return Err(no_such_method(method)) };
    if !token.has(def.scope) {
        return Err(RpcError::scope(method, def.scope.as_str(), &token.scope_names()));
    }
    if st.tokens.revoked(token) {
        return Err(crate::control::call::revoked(token));
    }
    match method {
        m if m.starts_with("token.") => crate::devices::call(&st.tokens, m, params).await,
        m if m.starts_with("show.") => super::shows_call::call(st, m, params).await,
        m if m.starts_with("channel.") => super::channel_calls::call(st, token, m, params).await,
        "governor.status" => {
            let mut status = status::governor_status(&st.render, &[]);
            status.ingress_kbps = ingress_kbps(st);
            status.cpu.measured_millicores = measured(st).await;
            Ok(serde_json::to_value(status).unwrap_or_default())
        }
        "governor.calibrate" => calibrate(st, &params),
        "network.share" => {
            let here = crate::control::methods::network::Here { open: st.tokens.is_open(), quit: &st.quit };
            let dry_run = params.get("dry_run").and_then(Value::as_bool).unwrap_or(false);
            crate::control::methods::network::share(here, params, dry_run).await
        }
        "rendition.plan" => plan(st, &params),
        "task.get" | "task.cancel" => {
            let req: crate::control::methods::task_request::TaskRequest = serde_json::from_value(params)
                .map_err(|e| RpcError::invalid_params(format!("{method} could not read its params: {e}")))?;
            super::switch::task::call(st, method, &req.task_id)
        }
        other => Err(no_such_method(other)),
    }
}

/// A method the table does not have: -32601, as a single process core
/// answers it, so a client tells a missing method from a missing show.
pub fn no_such_method(method: &str) -> RpcError {
    let near: Vec<String> = registry().nearest(method).iter().map(|s| s.to_string()).collect();
    let nearest = if near.is_empty() { String::new() } else { format!("Nearest: {}. ", near.join(", ")) };
    RpcError::new(
        godwinmix_protocol::ErrorCode::MethodNotFound,
        format!("there is no method '{method}'. {nearest}Call core.api for the whole list."),
    )
    .with("method", method)
    .with("nearest", near)
}

/// What arrives on this machine: the channels' live streams and the
/// direct shows' inputs, as last counted.
fn ingress_kbps(st: &Station) -> u32 {
    let channels = st.channels.get().map(|c| c.ingress_kbps()).unwrap_or(0);
    let direct: u32 = st.direct.seen.lock().values().filter_map(|s| s.input_stats.as_ref()).map(|i| i.kbps).sum();
    channels.saturating_add(direct)
}

/// What the station's work costs, read now because a client asked: the
/// station's own process, as the governor samples it, and every show
/// process and plugin it started, the direct host among them. The shows'
/// reports are taken out of the governor's figure first, since the reading
/// of the children counts those processes already. None where another
/// process's CPU cannot be read.
async fn measured(st: &Arc<Station>) -> Option<u32> {
    let children = super::usage::children(st).await.millicores()?;
    let reported: u32 = st.loads.lock().values().sum();
    let own = st.render.governor().load().own_millicores.saturating_sub(reported);
    Some(own.saturating_add(children))
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
        assert!(answers("network.share", &json!({"enabled": true})), "the station owns the port");
        assert!(answers("rendition.plan", &json!({"scope": "channel:church"})));
        assert!(!answers("rendition.plan", &json!({})));
        assert!(!answers("program.take", &json!({})));
        assert!(!answers("core.subscribe", &json!({})));
        assert!(answers("task.get", &json!({"id": "show-set-3"})), "a switch is the station's task");
        assert!(answers("task.get", &json!({"task_id": "show-set-3"})));
        assert!(!answers("task.get", &json!({"id": "plugin-add-1"})), "a show's task goes to the show");
    }

    #[test]
    fn a_method_the_table_does_not_have_is_method_not_found_with_the_nearest() {
        let e = no_such_method("show.statz");
        assert_eq!(e.code, -32601);
        assert!(e.data["nearest"].as_array().unwrap().iter().any(|n| n == "show.list"), "{:?}", e.data);
    }
}
