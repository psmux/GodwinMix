//! Changing what is said about an item, copying one, deleting one, and
//! drawing again every source that shows one that changed.

use super::super::{blocking, dir, entry, placed, words};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::gallery::{self, manifest::to_toml_table, Entry};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.edit", Scope::Operate, "Change an item's name, tags, description, zone or the values it fills its fields with.", handler(edit))
            .params(schema_of::<GalleryEditRequest>)
            .result(schema_of::<GallerySaved>)
            .tool("edit_graphic", Tier::Search, "Change a gallery graphic's name, tags, description, zone, or its field values (the words in a lower third). Sources showing it are drawn again. Shipped graphics are copied first with duplicate_graphic."),
    );
    reg.register(
        MethodDef::new("gallery.duplicate", Scope::Operate, "Copy an item, shipped ones included, under a new name.", handler(duplicate))
            .params(schema_of::<GalleryDuplicateRequest>)
            .result(schema_of::<GallerySaved>)
            .not_idempotent()
            .tool("duplicate_graphic", Tier::Search, "Copy a gallery graphic under a new name, to change it without touching the original. Works on the shipped pack too."),
    );
    reg.register(
        MethodDef::new("gallery.remove", Scope::Operate, "Delete a saved item and its files. Refused while a source shows it.", handler(remove))
            .params(schema_of::<GalleryIdRequest>)
            .destructive()
            .tool("remove_graphic", Tier::Search, "Delete a graphic from the gallery. Refused while a source on the mixer shows it; remove that source first. dry_run true says what it would delete."),
    );
}

async fn edit(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryEditRequest = call.params(&params)?;
    let zone = match req.zone.as_deref().map(str::trim).filter(|z| !z.is_empty()) {
        Some(z) => Some(Zone::parse(z).ok_or_else(|| RpcError::invalid_params(format!("zone {z:?} is not a zone. Use full, lower-third, bug, top, bottom, center or overlay."))).map(|z| z.as_str().to_string())?),
        None => None,
    };
    let id = entry(&req.id).await?.item.id;
    let r = req.clone();
    let edited = blocking("changing the item", move || {
        gallery::edit::edit(&dir(), &id, |m| {
            if let Some(n) = r.name.as_deref().map(str::trim).filter(|n| !n.is_empty()) {
                m.name = n.to_string();
            }
            if let Some(d) = &r.description {
                m.description = d.trim().to_string();
            }
            if r.tags.is_some() {
                m.tags = words(r.tags.as_ref());
            }
            if zone.is_some() {
                m.zone = zone.clone();
            }
            for (k, v) in r.values.iter().flatten() {
                m.values.remove(k);
                if !v.is_null() {
                    m.values.extend(to_toml_table(&serde_json::Map::from_iter([(k.clone(), v.clone())])));
                }
            }
        })
    })
    .await?
    .map_err(|e| RpcError::not_in_state(format!("{e:#}")).with("next", "duplicate_graphic"))?;
    answer(&call, edited, true).await
}

async fn duplicate(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryDuplicateRequest = call.params(&params)?;
    super::uploads_allowed(&call)?;
    let id = entry(&req.id).await?.item.id;
    let name = req.name.clone();
    let copy = blocking("copying the item", move || gallery::edit::duplicate(&dir(), &id, name.as_deref()))
        .await?
        .map_err(|e| RpcError::invalid_params(format!("{e:#}")))?;
    answer(&call, copy, false).await
}

async fn answer(call: &Call, mut e: Entry, redraw_it: bool) -> Result<Value, RpcError> {
    gallery::preview::forget(&dir(), &e.item.id);
    let redrawn = if redraw_it { redraw(call, &e).await } else { Vec::new() };
    placed(call, std::slice::from_mut(&mut e)).await;
    let path = e.dir().map(|d| d.display().to_string()).unwrap_or_default();
    let next = format!("preview_graphic {{\"id\": \"{}\"}} shows it.", e.item.id);
    body(GallerySaved { item: e.item, path, redrawn, warnings: Vec::new(), next })
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryIdRequest = call.params(&params)?;
    let mut e = entry(&req.id).await?;
    placed(&call, std::slice::from_mut(&mut e)).await;
    if !e.item.placed.is_empty() {
        return Err(RpcError::not_in_state(format!(
            "{} is shown by the source {}. Remove that source first (remove_source), then delete the graphic.",
            e.item.id,
            e.item.placed.join(", ")
        ))
        .with("sources", json!(e.item.placed)));
    }
    if call.dry_run {
        return Ok(call.dry_run_answer(!e.read_only(), vec![format!("delete {} and its files", e.item.id)]));
    }
    let id = e.item.id.clone();
    let gone = blocking("deleting the item", move || gallery::store::remove(&dir(), &id)).await?.map_err(|err| RpcError::not_in_state(format!("{err:#}")))?;
    body(json!({"id": e.item.id, "removed": gone.display().to_string()}))
}

/// Every source showing `e`, given its own params again so it reads the
/// changed file and draws it in place.
pub(super) async fn redraw(call: &Call, e: &Entry) -> Vec<String> {
    let Some(uri) = e.item.uri.as_deref() else { return Vec::new() };
    let Ok(configs) = call.app.mixer.configs().await else { return Vec::new() };
    let mut redrawn = Vec::new();
    for cfg in configs.sources.into_iter().filter(|s| s.uri.eq_ignore_ascii_case(uri)) {
        if call.app.mixer.configure_source(cfg.id.clone(), cfg.params.clone()).await.is_ok() {
            redrawn.push(cfg.id);
        }
    }
    redrawn
}
