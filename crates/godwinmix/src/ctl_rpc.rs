//! `gmx ctl rpc <method> [params]`: any method in the table, by name.
//!
//! The same method table the web UI and the MCP server use, so anything an
//! agent can call a person can call from a terminal, including the methods
//! no subcommand wraps. The params are JSON on the command line, or `@file`
//! to read them from a file.

use super::Api;
use anyhow::{bail, Context, Result};
use godwinmix_protocol::method::{rest_transform, Rest};
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

/// Where `method` is over plain HTTP: the route the method table gives it,
/// which is the one the server routes by. The transform rule is a guess only
/// for a name the table does not have, and a method the table keeps off
/// HTTP (`core.subscribe`) has no route at all.
pub fn rest_of(method: &str) -> Option<Rest> {
    match crate::station::methods::registry().get(method) {
        Some(def) => def.rest.clone(),
        None => rest_transform(method),
    }
}

/// Call `method` at its `/api/v1` route and hand back its answer. `/rpc`
/// is a WebSocket; over plain HTTP every method has a route of its own, the
/// one the method table gives it, with `id` (or `name`) in the path.
///
/// A name the table does not have is refused here with the methods nearest
/// to it, as `/rpc` refuses it, rather than sent to a path that does not
/// exist: `scene.take` was answered "there is no /api/v1/scenes/x/take".
pub async fn call(api: &Api, method: &str, params: Value) -> Result<Value> {
    let Value::Object(mut params) = params else { bail!("the params of {method} are a JSON object") };
    let templated = route_for(method)?.path.contains("{id}");
    let id = if templated { Some(path_id(method, &mut params)?) } else { None };
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

/// What a route with `{id}` in it acts on, taken out of the params: `id`,
/// or the key the method's own params name it by (`scene` for `scene.*`,
/// `task_id` for `task.*`), or a `name` or `source`. The key the core reads
/// as another name for `id` leaves the body, since the core refuses the two
/// together as a duplicate field; `name` and `source` stay, as they may be
/// fields of their own.
fn path_id(method: &str, params: &mut serde_json::Map<String, Value>) -> Result<String> {
    let noun = method.split('.').next().unwrap_or(method);
    let (named, owned) = (noun.to_string(), format!("{noun}_id"));
    let keys = ["id", named.as_str(), owned.as_str(), "name", "source"];
    let Some((at, id)) = keys.iter().enumerate().find_map(|(n, k)| params.get(*k).and_then(Value::as_str).map(|v| (n, v.to_string()))) else {
        bail!("{method} names what it acts on in its path: pass \"id\" (or \"{noun}\") in the params");
    };
    if at < 3 {
        params.remove(keys[at]);
    }
    Ok(id)
}

/// The method's route, or a refusal that names the next step: the methods
/// nearest to an unknown name, or `/rpc` for a method HTTP does not carry.
fn route_for(method: &str) -> Result<Rest> {
    if crate::station::methods::registry().get(method).is_none() {
        let e = crate::station::methods::no_such_method(method);
        bail!("{}\n{}", e.message, serde_json::to_string_pretty(&e.data)?);
    }
    rest_of(method).with_context(|| format!("{method} is not on /api/v1: call it over the /rpc WebSocket"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unknown_method_is_refused_with_the_one_that_does_it() {
        let e = route_for("scene.take").unwrap_err().to_string();
        assert!(e.contains("there is no method 'scene.take'"), "{e}");
        assert!(e.contains("Nearest: program.take"), "program.take comes first: {e}");
    }

    #[test]
    fn the_member_is_read_by_the_name_the_method_gives_it() {
        let take = |method: &str, v: Value| {
            let Value::Object(mut map) = v else { unreachable!() };
            path_id(method, &mut map).map(|id| (id, Value::Object(map)))
        };
        // The how to guides write `scene`, as the method's own params do.
        let (id, rest) = take("scene.item.set", json!({"scene": "studio", "item": "strap"})).unwrap();
        assert_eq!((id.as_str(), rest), ("studio", json!({"item": "strap"})));
        let (id, rest) = take("scene.rename", json!({"scene": "a", "name": "B"})).unwrap();
        assert_eq!((id.as_str(), rest), ("a", json!({"name": "B"})), "a new name is not the member");
        assert_eq!(take("task.get", json!({"task_id": "t-1"})).unwrap().0, "t-1");
        assert_eq!(take("source.get", json!({"id": "cam1"})).unwrap(), ("cam1".into(), json!({})));
        let e = take("scene.get", json!({"item": 1})).unwrap_err().to_string();
        assert!(e.contains("\"scene\""), "{e}");
    }

    #[test]
    fn a_method_goes_to_the_route_the_server_has_for_it() {
        // Routes the transform rule gets wrong, which the table writes out.
        assert_ne!(rest_transform("snapshot.get").unwrap().path, "/api/v1/snapshot/{id}");
        assert_eq!(route_for("snapshot.get").unwrap().path, "/api/v1/snapshot/{id}");
        assert_eq!(route_for("media.upload").unwrap().path, "/api/v1/media/upload");
        assert!(route_for("core.subscribe").unwrap_err().to_string().contains("/rpc"));
        assert_eq!(route_for("source.get").unwrap().path, "/api/v1/sources/{id}");
    }

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
