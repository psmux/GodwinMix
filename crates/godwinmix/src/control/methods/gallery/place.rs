//! `gallery.place`: an item onto a scene, in its zone, hidden until shown.
//!
//! One source per item, named by the item's id, so placing the same item
//! on a second scene draws the one source twice rather than decoding it
//! twice. Placing it again on the same scene changes its words and nothing
//! else, which is how a model updates a lower third without knowing about
//! sources at all. A set is a scene of its own, made by `scene.create_from`.

#[path = "scene_of.rs"]
pub(super) mod scene_of;
#[path = "place_set.rs"]
mod set;

use super::entry;
use crate::control::call::Call;
use crate::control::methods::project::invoke;
use crate::control::methods::{body, handler};
use godwinmix_core::gallery::{place, Entry};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Map, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.place", Scope::Operate, "Add a gallery item to a scene in its zone (a lower third low on the left, a background under everything, a bug in the corner), hidden until gallery.show. A set becomes a new scene.", handler(place_call))
            .params(schema_of::<GalleryPlaceRequest>)
            .result(schema_of::<GalleryPlaced>)
            .not_idempotent()
            .tool(
                "place_graphic",
                Tier::Search,
                "Put a gallery graphic on a scene, in the right place for its kind, hidden and ready. Give `id` \
                 from list_graphics; `values` sets its words (call again to change them); `scene` defaults to \
                 the one on air. For a virtual set it makes a new scene with `camera` standing in it. Then \
                 show_graphic puts it on air.",
            ),
    );
}

async fn place_call(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryPlaceRequest = call.params(&params)?;
    let e = entry(&req.id).await?;
    if e.item.kind.is_played() {
        return Err(RpcError::invalid_params(format!(
            "{} is a {}: it is played by a take, not placed on a scene. Choose it as the transition when you take, or fire it as an effect.",
            e.item.id,
            e.item.kind.as_str()
        )));
    }
    if e.item.kind == GalleryKind::Set {
        return set::place(&call, &e, &req).await;
    }
    let zone = match req.zone.as_deref().map(str::trim).filter(|z| !z.is_empty()) {
        Some(z) => Zone::parse(z).ok_or_else(|| RpcError::invalid_params(format!("zone {z:?} is not a zone. Use full, lower-third, bug, top, bottom, center or overlay.")).with("field", "zone"))?,
        None => e.item.zone,
    };
    let source = source_for(&call, &e, req.values.as_ref()).await?;
    let target = scene_of::target(&call, req.scene.as_deref(), true).await?;
    let view = scene_of::view(&call, &target.id).await?;
    let visible = req.visible.unwrap_or(false);
    let name = e.item.name.clone();
    if let Some((item, ..)) = scene_of::items(&view).into_iter().find(|(_, _, s, _)| *s == source) {
        if let Some(v) = req.visible {
            invoke(&call, "scene.item.set", json!({"scene": target.id, "item": item, "props": {"visible": v}})).await?;
        }
        return answer(&e, &target, Some(source), name, req.visible.unwrap_or(false), true);
    }
    let canvas = (call.app.canvas.width.max(2) as u32, call.app.canvas.height.max(2) as u32);
    let natural = natural_of(&e);
    let added = invoke(&call, "scene.item.add", json!({
        "scene": target.id, "content": {"source": source}, "name": name,
        "transform": place::transform(zone, canvas, natural), "visible": visible,
        "enter": place::enter(zone), "exit": place::exit(zone),
    }))
    .await?;
    if place::underneath(zone) {
        if let (Some(below), Some(me)) = (scene_of::bottom(&view), added.get("id").or_else(|| added.pointer("/item/id"))) {
            let me = me.as_str().map(str::to_string).unwrap_or_else(|| me.to_string());
            let _ = invoke(&call, "scene.item.reorder", json!({"scene": target.id, "item": me, "before": below})).await;
        }
    }
    answer(&e, &target, Some(source), name, visible, false)
}

fn answer(e: &Entry, t: &scene_of::Target, source: Option<String>, item: String, visible: bool, updated: bool) -> Result<Value, RpcError> {
    let next = if visible {
        format!("It is showing on {}. show_graphic {{\"id\": \"{}\", \"visible\": false}} takes it off.", t.name, e.item.id)
    } else {
        format!("It is on {} and hidden. show_graphic {{\"id\": \"{}\"}} puts it on air.", t.name, e.item.id)
    };
    body(GalleryPlaced { id: e.item.id.clone(), scene: t.name.clone(), source, item: Some(item), visible, updated, new_scene: t.made, next })
}

/// The size an item was made at, for its box.
fn natural_of(e: &Entry) -> Option<(u32, u32)> {
    match e.item.kind {
        GalleryKind::Template | GalleryKind::Html | GalleryKind::Ograf => Some((1920, 1080)),
        _ => e.file().and_then(|f| godwinmix_core::gallery::detect::size(&f)),
    }
}

/// The source drawing `e`: made when there is none, its field values set
/// when there is.
async fn source_for(call: &Call, e: &Entry, values: Option<&Map<String, Value>>) -> Result<String, RpcError> {
    let uri = uri_of(e)?;
    let mut params = Map::new();
    match e.item.kind {
        GalleryKind::Template => {
            let mut fields = e.item.values.clone();
            fields.extend(values.cloned().unwrap_or_default());
            params.insert("fields".into(), Value::Object(fields));
        }
        GalleryKind::Ticker | GalleryKind::Text => {
            params = godwinmix_core::gallery::manifest::to_json_map(&e.manifest.source.as_ref().map(|s| s.params.clone()).unwrap_or_default());
            params.extend(values.cloned().unwrap_or_default());
        }
        _ => {}
    }
    let configs = call.app.mixer.configs().await.map_err(|err| call.mixer_error(err))?;
    let id = e.item.id.clone();
    if let Some(same) = configs.sources.iter().find(|s| s.id == id || s.uri.eq_ignore_ascii_case(&uri)) {
        if same.uri.eq_ignore_ascii_case(&uri) {
            if values.is_some() && !params.is_empty() {
                invoke(call, "source.set", json!({"id": same.id, "params": params})).await?;
            }
            return Ok(same.id.clone());
        }
    }
    let free = (1..).map(|n| if n == 1 { id.clone() } else { format!("{id}-{n}") }).find(|c| !configs.sources.iter().any(|s| &s.id == c)).unwrap_or(id);
    let added = invoke(call, "source.add", json!({"id": free, "name": e.item.name, "uri": uri, "params": params})).await?;
    Ok(added.get("id").and_then(Value::as_str).unwrap_or(&free).to_string())
}

/// The address a source draws `e` from.
pub(super) fn uri_of(e: &Entry) -> Result<String, RpcError> {
    if let Some(uri) = &e.item.uri {
        return Ok(uri.clone());
    }
    match e.item.kind {
        GalleryKind::Html | GalleryKind::Ograf => {
            let addr = crate::control::bound().ok_or_else(|| RpcError::not_in_state("the control port is not listening yet; try again in a moment"))?;
            let page = match e.item.kind {
                GalleryKind::Html if !e.manifest.file.is_empty() => e.manifest.file.clone(),
                _ => "index.html".to_string(),
            };
            if !e.dir().is_some_and(|d| d.join(&page).is_file()) {
                return Err(RpcError::not_in_state(format!(
                    "{} is an OGraf package with no index.html, and an OGraf web component is drawn by the OGraf plugin. \
                     Install it (gmx plugin add ./plugins/ograf) and add the package to it, or save the graphic as a whole HTML page.",
                    e.item.id
                )));
            }
            Ok(format!("web+http://{addr}/api/v1/gallery/{}/files/{page}", e.item.id))
        }
        k => Err(RpcError::invalid_params(format!("{} is a {} and has no file a source can read", e.item.id, k.as_str()))),
    }
}
