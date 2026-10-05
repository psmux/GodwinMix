//! Which scene a graphic goes on, and what is on it already.
//!
//! Asked for by name, that one. Otherwise the scene on air, then the armed
//! one. A mixer showing a plain source with no scene gets a scene made from
//! that source, so placing a graphic never changes the picture until the
//! graphic is shown.

use crate::control::call::Call;
use crate::control::methods::project::invoke;
use godwinmix_protocol::error::RpcError;
use serde_json::{json, Value};

/// The scene's id and name, and whether it was made just now.
pub(crate) struct Target {
    pub id: String,
    pub name: String,
    pub made: bool,
}

pub(crate) async fn target(call: &Call, asked: Option<&str>, make: bool) -> Result<Target, RpcError> {
    if let Some(asked) = asked.map(str::trim).filter(|s| !s.is_empty()) {
        let view = invoke(call, "scene.get", json!({"scene": asked})).await?;
        return Ok(of(&view, false));
    }
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    let scenes = call.app.scenes.list();
    // On air, then armed, then a scene that is the source on air and nothing
    // else (what a take of a source looks like), so a graphic goes over the
    // picture people are watching without a new scene each time.
    let alone = |x: &&godwinmix_core::scene::server::SceneSummary| status.program.as_deref().is_some_and(|p| x.sources.len() == 1 && x.sources[0] == p);
    let with_graphics = |x: &&godwinmix_core::scene::server::SceneSummary| status.program.as_deref().is_some_and(|p| x.name == format!("{p} with graphics"));
    let pick = status
        .scene
        .as_deref()
        .and_then(|s| scenes.iter().find(|x| x.name == s || x.id.to_string() == s))
        .or_else(|| scenes.iter().find(|x| x.armed))
        .or_else(|| scenes.iter().find(with_graphics))
        .or_else(|| scenes.iter().find(alone));
    if let Some(s) = pick {
        return Ok(Target { id: s.id.to_string(), name: s.name.clone(), made: false });
    }
    if !make {
        return Err(RpcError::not_in_state("no scene is on air or armed. Name one in `scene` (list_scenes has them), or place the graphic first with place_graphic."));
    }
    // `scene.create_from` answers with the scene itself, flattened.
    let view = match status.program.as_deref() {
        Some(source) => invoke(call, "scene.create_from", json!({"sources": [source], "name": format!("{source} with graphics")})).await?,
        None => invoke(call, "scene.add", json!({"name": "Graphics"})).await?,
    };
    Ok(of(&view, true))
}

fn of(view: &Value, made: bool) -> Target {
    let s = |k: &str| view.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
    Target { id: s("id"), name: s("name"), made }
}

/// The scene as JSON.
pub(crate) async fn view(call: &Call, scene: &str) -> Result<Value, RpcError> {
    invoke(call, "scene.get", json!({"scene": scene})).await
}

/// The items of a scene: (id, name, source it draws, visible).
pub(crate) fn items(view: &Value) -> Vec<(String, String, String, bool)> {
    view["records"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["kind"] == "item")
        .map(|r| {
            let s = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
            (s(&r["id"]), r["name"].as_str().unwrap_or_default().to_string(), r["content"]["source"].as_str().unwrap_or_default().to_string(), r["visible"].as_bool().unwrap_or(true))
        })
        .collect()
}

/// The bottom item of a scene, which a background goes behind.
pub(crate) fn bottom(view: &Value) -> Option<String> {
    let first = view["geometry"].as_array()?.first()?;
    first["item"].as_str().map(str::to_string).or_else(|| Some(first["item"].to_string()))
}

/// True when this scene is the one on air.
pub(crate) async fn on_air(call: &Call, t: &Target) -> bool {
    match call.app.mixer.status().await {
        Ok(s) => s.scene.as_deref().is_some_and(|x| x == t.name || x == t.id),
        Err(_) => false,
    }
}
