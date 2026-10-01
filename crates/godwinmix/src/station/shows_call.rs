//! The station's answer to `show.*`, by name.

use super::state::Station;
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};
use std::sync::Arc;

fn parse<T: serde::de::DeserializeOwned>(method: &str, params: Value) -> Result<T, RpcError> {
    serde_json::from_value(params).map_err(|e| RpcError::invalid_params(format!("{method}: {e}")))
}

/// The station's answer to `show.*`, by name.
pub async fn call(st: &Arc<Station>, method: &str, params: Value) -> Result<Value, RpcError> {
    let id = || params.get("id").and_then(Value::as_str).map(str::to_string).ok_or_else(|| RpcError::invalid_params("name the show with id").with("field", "id"));
    match method {
        "show.list" => Ok(serde_json::to_value(st.list().await).unwrap_or_default()),
        "show.add" => super::shows_api::add(st, parse(method, params.clone())?).await,
        "show.rename" => super::shows_api::rename(st, parse(method, params.clone())?),
        "show.remove" => super::shows_api::remove(st, &id()?).await,
        "show.start" => super::shows_api::start(st, &id()?).await,
        "show.stop" => super::shows_api::stop(st, &id()?).await,
        "show.set" => super::shows_set::set(st, parse(method, params)?).await,
        "show.add_many" => super::shows_bulk::add_many(st, parse(method, params)?).await,
        "show.remove_many" => super::shows_bulk::remove_many(st, parse(method, params)?).await,
        "show.stats" => super::shows_stats::stats(st, parse(method, params)?),
        m if m.starts_with("show.output.") => super::shows_set::output(st, m, params),
        other => Err(RpcError::not_found("method", other, &[]).with("hint", json!("show.list, show.add, show.add_many, show.set, show.rename, show.remove, show.remove_many, show.start, show.stop, show.stats, show.output.add, show.output.set, show.output.remove"))),
    }
}
