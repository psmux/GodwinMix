//! A value into its target, through the method a client would call.
//!
//! `source.set`, `scene.apply_graphic` and `scene.params.set` are taken from
//! the same method table `/rpc` serves and their handlers run as they are,
//! so a binding can do nothing a client could not and a text still changes
//! in place. What is skipped is the envelope around a client's call: a feed
//! is not an operator, so it does not keep the operator watchdog quiet, and
//! a write every few seconds does not fill the session log.

use super::{check, Ctx};
use crate::control::call::Call;
use godwinmix_protocol::feeds::BindingTarget;
use godwinmix_protocol::method::Registry;
use godwinmix_protocol::scope::{Scope, Token};
use serde_json::{json, Map, Value};
use std::sync::OnceLock;
use std::time::Duration;

const WRITE_TIMEOUT: Duration = Duration::from_secs(10);

pub async fn to(ctx: &Ctx, target: &BindingTarget, value: &Value) -> Result<(), String> {
    let (method, params) = match target {
        BindingTarget::Source { source, path } => ("source.set", source_params(ctx, source, path, value).await?),
        BindingTarget::Graphic { graphic, field, item } => {
            let mut p = json!({ "graphic": graphic, "values": { field.as_str(): value } });
            if let Some(item) = item {
                p["item"] = json!(item);
            }
            ("scene.apply_graphic", p)
        }
        BindingTarget::SceneParam { scene_param } => ("scene.params.set", json!({ "values": { scene_param.as_str(): value } })),
    };
    call(ctx, method, params).await.map(|_| ())
}

/// `{id, params: {first_key: value}}`, with a nested path merged into what
/// the source has under its first key, so `params.fields.headline` changes
/// one field and leaves the others.
async fn source_params(ctx: &Ctx, source: &str, path: &str, value: &Value) -> Result<Value, String> {
    let keys = check::param_keys(path).map_err(|e| e.message)?;
    let (first, rest) = keys.split_first().ok_or("the param path is empty")?;
    if rest.is_empty() {
        return Ok(json!({ "id": source, "params": { first.as_str(): value } }));
    }
    let configs = ctx.app.mixer.configs().await.map_err(|e| format!("{e:#}"))?;
    let current = configs
        .sources
        .iter()
        .find(|s| s.id == source)
        .and_then(|s| s.params.get(first))
        .and_then(|v| serde_json::to_value(v).ok())
        .unwrap_or(Value::Null);
    Ok(json!({ "id": source, "params": { first.as_str(): set_in(current, rest, value.clone()) } }))
}

/// `value` at `keys` inside `base`, making objects on the way.
fn set_in(base: Value, keys: &[String], value: Value) -> Value {
    let Some((key, rest)) = keys.split_first() else { return value };
    let mut map = match base {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    let inner = map.remove(key).unwrap_or(Value::Null);
    map.insert(key.clone(), set_in(inner, rest, value));
    Value::Object(map)
}

fn registry() -> &'static Registry<Call> {
    static REG: OnceLock<Registry<Call>> = OnceLock::new();
    REG.get_or_init(crate::control::methods::registry)
}

/// The token a binding writes as: operate, and named so `program.history`
/// and a log line say a feed did it.
fn token() -> Token {
    Token { id: "feed".into(), scopes: vec![Scope::Read, Scope::Operate], ..Token::open() }
}

async fn call(ctx: &Ctx, method: &'static str, params: Value) -> Result<Value, String> {
    let def = registry().get(method).ok_or_else(|| format!("this core has no {method}"))?;
    let call = Call {
        app: ctx.app.clone(),
        snapshots: ctx.snapshots.clone(),
        token: token(),
        trace_id: format!("feed-{method}"),
        dry_run: false,
        method: def.name,
    };
    match tokio::time::timeout(WRITE_TIMEOUT, (def.handler)(call, params)).await {
        Ok(Ok(v)) => Ok(v),
        Ok(Err(e)) => Err(format!("{method} refused it: {}", e.message)),
        Err(_) => Err(format!("{method} did not answer within {} s; the mixer is busy, and it is tried again", WRITE_TIMEOUT.as_secs())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_path_keeps_the_fields_beside_it() {
        let base = json!({ "headline": "old", "kicker": "News" });
        let got = set_in(base, &["headline".into()], json!("new"));
        assert_eq!(got, json!({ "headline": "new", "kicker": "News" }));
        assert_eq!(set_in(Value::Null, &["a".into(), "b".into()], json!(1)), json!({ "a": { "b": 1 } }));
    }
}
