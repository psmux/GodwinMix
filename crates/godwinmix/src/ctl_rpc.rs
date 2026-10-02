//! `gmx ctl rpc <method> [params]`: any method in the table, by name.
//!
//! The same method table the web UI and the MCP server use, so anything an
//! agent can call a person can call from a terminal, including the methods
//! no subcommand wraps. The params are JSON on the command line, or `@file`
//! to read them from a file.

use super::Api;
use anyhow::{bail, Context, Result};
use godwinmix_protocol::method::rest_transform;
use serde_json::{json, Value};

/// Read the params: JSON, `@path` for a JSON file, or nothing for `{}`.
pub fn params(raw: Option<&str>) -> Result<Value> {
    let Some(raw) = raw.map(str::trim).filter(|r| !r.is_empty()) else { return Ok(json!({})) };
    let text = match raw.strip_prefix('@') {
        Some(path) => std::fs::read_to_string(path).with_context(|| format!("reading the params from {path}"))?,
        None => raw.to_string(),
    };
    serde_json::from_str(&text).with_context(|| format!("the params are not JSON: {text}. Write them as '{{\"id\": \"cam1\"}}'"))
}

/// Call `method` and print its answer, or its refusal with the data a
/// caller acts on.
pub async fn run(api: &Api, method: &str, params: Value) -> Result<()> {
    let answer = call(api, method, params).await?;
    println!("{}", serde_json::to_string_pretty(&answer)?);
    Ok(())
}

/// Call `method` at its `/api/v1` route and hand back its answer. `/rpc`
/// is a WebSocket; over plain HTTP every method has a route of its own, the
/// one the transform rule gives it, with `id` (or `name`) in the path.
pub async fn call(api: &Api, method: &str, params: Value) -> Result<Value> {
    let Value::Object(mut params) = params else { bail!("the params of {method} are a JSON object") };
    let templated = rest_transform(method).is_some_and(|r| r.path.contains("{id}"));
    let id = templated.then(|| ["id", "name", "source"].iter().find_map(|k| params.get(*k).and_then(Value::as_str).map(String::from))).flatten();
    if templated && id.is_none() {
        bail!("{method} names what it acts on in its path: pass \"id\" in the params");
    }
    if id.is_some() {
        params.remove("id");
    }
    let (verb, url) = api.route(method, id.as_deref())?;
    let req = api.client.request(verb.clone(), &url);
    let req = if verb == reqwest::Method::GET {
        let pairs: Vec<(String, String)> = params.iter().map(|(k, v)| (k.clone(), v.as_str().map(String::from).unwrap_or_else(|| v.to_string()))).collect();
        req.query(&pairs)
    } else {
        req.json(&params)
    };
    let r = req.send().await.with_context(|| format!("calling {method} at {url}"))?;
    let ok = r.status().is_success();
    let text = r.text().await.unwrap_or_default();
    let answer: Value = serde_json::from_str(text.trim()).unwrap_or(if text.trim().is_empty() { Value::Null } else { Value::String(text) });
    if !ok {
        let e = &answer["error"];
        let message = e["message"].as_str().unwrap_or("refused");
        bail!("{method}: {message}\n{}", serde_json::to_string_pretty(&e["data"])?);
    }
    Ok(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn params_are_json_a_file_or_nothing() {
        assert_eq!(params(None).unwrap(), json!({}));
        assert_eq!(params(Some(r#"{"id":"cam1"}"#)).unwrap(), json!({"id": "cam1"}));
        let path = std::env::temp_dir().join(format!("gmx-rpc-{}.json", std::process::id()));
        std::fs::write(&path, r#"{"name":"x"}"#).unwrap();
        assert_eq!(params(Some(&format!("@{}", path.display()))).unwrap(), json!({"name": "x"}));
        let _ = std::fs::remove_file(&path);
        assert!(params(Some("not json")).unwrap_err().to_string().contains("not JSON"));
    }
}
