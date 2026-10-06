//! Changing an item, taking one out, and its preview strip.

use super::*;

pub(super) async fn set(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxSetRequest = call.params(&params)?;
    let dir = media(&call);
    let entry = blocking(move || {
        let (mut m, folder) = library::find(&dir, &req.name)?;
        if let Some(t) = req.title {
            m.title = t;
        }
        m.blend = req.blend.unwrap_or(m.blend);
        if let Some(ms) = req.cut_at_ms {
            m.cut_at_ms = (ms > 0).then_some(ms.min(m.duration_ms));
        }
        if let (Some(ms), FxKind::Matte | FxKind::Shader) = (req.duration_ms, m.kind) {
            m.duration_ms = ms.clamp(1, godwinmix_core::mixer::transition::MAX_DURATION_MS);
        }
        m.softness = req.softness.map(|s| s.clamp(0.0, 1.0)).or(m.softness);
        m.invert = req.invert.unwrap_or(m.invert);
        m.transition = req.transition.unwrap_or(m.transition);
        m.effect = req.effect.unwrap_or(m.effect) && matches!(m.kind, FxKind::Stinger | FxKind::Overlay);
        library::save(&folder, &m)?;
        Ok(library::entry(&m, &folder))
    })
    .await?;
    body(entry)
}

pub(super) async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxNameRequest = call.params(&params)?;
    let dir = media(&call);
    let name = req.name.clone();
    blocking(move || library::remove(&dir, &req.name)).await?;
    body(json!({ "removed": name }))
}

pub(super) async fn preview(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxNameRequest = call.params(&params)?;
    let dir = media(&call);
    let out = blocking(move || {
        let (m, folder) = library::find(&dir, &req.name)?;
        fx::sprite::ensure(&m, &folder)?;
        Ok(m)
    })
    .await?;
    let (w, h) = fx::detect::SMALL;
    body(FxPreview { url: format!("/api/v1/fx/{}/preview.jpg", out.name), name: out.name, frames: fx::sprite::FRAMES, frame_width: w, frame_height: h, duration_ms: out.duration_ms })
}

/// `fx.assign`: the transition a take uses when it names none, for a scene
/// or for every take. The name is checked against everything a take accepts.
pub(super) async fn assign(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: FxAssignRequest = call.params(&params)?;
    if let Some(name) = req.transition.as_deref().map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty()) {
        let mut names: Vec<String> = godwinmix_protocol::requests::TRANSITIONS.iter().map(|s| s.to_string()).collect();
        names.extend(call.app.scenes.transition_names());
        names.extend(call.app.transition_names());
        names.extend(take::transition_names(&call));
        if !names.contains(&name) {
            return Err(RpcError::invalid_params(format!("no transition called {name:?}. Use one of: {}", names.join(", "))).with("transitions", json!(names)));
        }
    }
    if let Some(scene) = &req.scene {
        call.app.scenes.scene(scene).map_err(|e| crate::control::methods::scenes::scene_error(&call, e))?;
    }
    let dir = media(&call);
    let assigned = blocking(move || fx::assign::set(&dir, req.scene.as_deref(), req.transition.as_deref())).await?;
    body(assigned)
}
