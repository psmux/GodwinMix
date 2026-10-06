//! A set placed is a new scene, made by `scene.create_from` with the set's
//! pictures, the camera standing in it and the layout's settings. There is
//! no virtual set feature apart from scenes; this only fills in the call.

use crate::control::call::Call;
use crate::control::methods::body;
use crate::control::methods::project::invoke;
use godwinmix_core::gallery::{entry::plain, manifest::to_json_map, Entry};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use serde_json::{json, Value};

pub(super) async fn place(call: &Call, e: &Entry, req: &GalleryPlaceRequest) -> Result<Value, RpcError> {
    let spec = e.manifest.set.as_ref().ok_or_else(|| RpcError::internal(format!("{} is a set with no [set] in its graphic.toml", e.item.id)))?;
    let dir = e.dir().ok_or_else(|| RpcError::internal("a set with no folder"))?;
    let camera = camera(call, req.camera.as_deref()).await?;
    let mut sources = vec![drawn(&dir.join(&spec.background)), camera.clone()];
    if let Some(front) = spec.foreground.as_ref().filter(|f| dir.join(f).is_file()) {
        sources.push(drawn(&dir.join(front)));
    }
    let mut settings = to_json_map(&spec.settings);
    let screen = screen_for(call, &camera, req.screen.as_deref(), settings.get("screen")).await;
    settings.insert("screen".into(), Value::String(screen));
    let made = invoke(call, "scene.create_from", json!({
        "sources": sources,
        "layout": spec.layout.clone().unwrap_or_else(|| "virtual-set".into()),
        "name": e.item.name,
        "settings": settings,
    }))
    .await?;
    // The answer is the scene itself, flattened, with what was added beside it.
    let scene = made.get("name").and_then(Value::as_str).unwrap_or(&e.item.name).to_string();
    body(GalleryPlaced {
        id: e.item.id.clone(),
        scene: scene.clone(),
        source: None,
        item: None,
        visible: false,
        updated: false,
        new_scene: true,
        next: format!("The scene {scene} has {camera} standing in the set. show_graphic {{\"id\": \"{}\"}} takes it to air; place a lower third on it with place_graphic {{\"scene\": \"{scene}\"}}.", e.item.id),
    })
}

/// How a set's file is drawn: an HTML template (a moving backdrop) as
/// `html:`, an SVG with fields as `template:`, anything else by its path.
fn drawn(file: &std::path::Path) -> String {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let name = file.to_string_lossy().to_ascii_lowercase();
    if name.ends_with(".html") && text.contains(godwinmix_core::graphics::html::meta::BLOCK_ID) {
        format!("html:{}", plain(file))
    } else if name.ends_with(".svg") && (text.contains("{{") || text.contains("<gmx:template")) {
        format!("template:{}", plain(file))
    } else {
        plain(file)
    }
}

/// What the camera stands in front of: as asked, else as the set says, else
/// read off its picture. A set placed over a room with no green screen was
/// keyed green by default and showed the whole room behind the presenter;
/// with no screen to be found, the person is cut out instead.
async fn screen_for(call: &Call, camera: &str, asked: Option<&str>, set: Option<&Value>) -> String {
    if let Some(s) = asked.map(str::trim).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("auto")) {
        return s.to_ascii_lowercase();
    }
    if let Some(s) = set.and_then(Value::as_str).filter(|s| !s.eq_ignore_ascii_case("auto")) {
        return s.to_string();
    }
    match crate::control::methods::scenes::key_color::guess(call, camera).await {
        Some(found) => found.found,
        None => "none".into(),
    }
}

/// The camera for a set: the one asked for, else the source on air.
async fn camera(call: &Call, asked: Option<&str>) -> Result<String, RpcError> {
    if let Some(c) = asked.map(str::trim).filter(|c| !c.is_empty()) {
        return Ok(c.to_string());
    }
    let status = call.app.mixer.status().await.map_err(|e| call.mixer_error(e))?;
    status.program.clone().ok_or_else(|| {
        let ids: Vec<String> = status.sources.iter().map(|s| s.id.clone()).collect();
        RpcError::invalid_params(format!(
            "a set needs a camera to stand in it and nothing is on air to use. Give `camera` with a source id: {}.",
            if ids.is_empty() { "add one first with add_source".to_string() } else { ids.join(", ") }
        ))
        .with("field", "camera")
    })
}
