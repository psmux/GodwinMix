//! `gallery.show`: a placed graphic on air, or off, in one call.
//!
//! The item on its scene is shown (it comes in the way its `enter` says),
//! and when that scene is not the one on air it is taken, so "show the
//! lower third" means the audience sees it. A set's scene is taken.

use super::place::scene_of;
use crate::control::call::Call;
use crate::control::methods::project::invoke;
use crate::control::methods::{body, handler};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.show", Scope::Operate, "Show a placed gallery item on air, or hide it, taking its scene when that scene is not on air. For a set, take its scene.", handler(show))
            .params(schema_of::<GalleryShowRequest>)
            .result(schema_of::<GalleryShown>)
            .not_idempotent()
            .tool(
                "show_graphic",
                Tier::Search,
                "Put a placed gallery graphic on air (it animates in), or take it off with visible false. \
                 Give the same `id` you gave place_graphic. If its scene is not on air, the scene is taken too.",
            ),
    );
}

async fn show(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryShowRequest = call.params(&params)?;
    let visible = req.visible.unwrap_or(true);
    let gallery_item = super::entry(&req.id).await.ok();
    if let Some(e) = gallery_item.as_ref().filter(|e| e.item.kind == GalleryKind::Set) {
        let scene = req.scene.clone().unwrap_or_else(|| e.item.name.clone());
        invoke(&call, "program.take", json!({"scene": scene})).await.map_err(|err| err.with("next", "place_graphic makes the set's scene first"))?;
        return body(GalleryShown { scene, item: e.item.name.clone(), visible: true, took: true });
    }
    let target = scene_of::target(&call, req.scene.as_deref(), false).await?;
    let view = scene_of::view(&call, &target.id).await?;
    let items = scene_of::items(&view);
    let wanted: Vec<String> = [Some(req.id.trim().to_string()), gallery_item.as_ref().map(|e| e.item.id.clone()), gallery_item.as_ref().map(|e| e.item.name.clone())].into_iter().flatten().collect();
    let found = items.iter().find(|(id, name, source, _)| wanted.iter().any(|w| w == source || w == name || w == id || source.strip_prefix(w.as_str()).is_some_and(|r| r.starts_with('-') && r[1..].parse::<u32>().is_ok())));
    let Some((item, name, ..)) = found else {
        let names: Vec<&str> = items.iter().map(|(_, n, ..)| n.as_str()).collect();
        return Err(RpcError::not_found("graphic on the scene", &req.id, &names.iter().map(|s| s.to_string()).collect::<Vec<_>>())
            .with("scene", target.name.clone())
            .with("next", format!("place_graphic {{\"id\": \"{}\"}} puts it on {} first", req.id.trim(), target.name)));
    };
    invoke(&call, "scene.item.set", json!({"scene": target.id, "item": item, "props": {"visible": visible}})).await?;
    let took = visible && !scene_of::on_air(&call, &target).await;
    if took {
        invoke(&call, "program.take", json!({"scene": target.id})).await?;
    }
    body(GalleryShown { scene: target.name, item: name.clone(), visible, took })
}
