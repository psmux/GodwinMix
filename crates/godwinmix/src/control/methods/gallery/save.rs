//! `gallery.save`, `gallery.edit`, `gallery.duplicate`, `gallery.remove`:
//! putting items in, changing them and taking them out.

#[path = "input.rs"]
mod input;
#[path = "manage.rs"]
mod manage;

use super::{blocking, dir, words};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::gallery::{self, draft::Draft, edit::now, manifest::to_toml_table};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;

pub(crate) use input::decode;

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.save", Scope::Operate, "Save a graphic of any kind into the gallery with a name, tags and a description: an SVG template, an HTML page, a picture or clip, a ticker or text, or a virtual set.", handler(save))
            .params(schema_of::<GallerySaveRequest>)
            .result(schema_of::<GallerySaved>)
            .not_idempotent()
            .tool(
                "save_graphic",
                Tier::Search,
                "Save a graphic you made into the GodwinMix graphics gallery, in one call. Give `name` and \
                 ONE of: `svg` (an SVG; {{field}} markers make it a template), `html` (a web page, transparent \
                 background), `data` (a PNG, WebP or WebM as base64), `file` (a path), `source` (a ticker: \
                 {\"uri\":\"ticker:\",\"params\":{\"items\":[...]}}) or `set` (a virtual set: {\"background\":...,\"foreground\":...}). \
                 Optional: `tags`, `description`, `zone` (lower-third, full, bug, bottom, center). Then call \
                 preview_graphic to look at it. With `replace` true it writes over an item of the same name.",
            ),
    );
    manage::register(reg);
}

async fn save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GallerySaveRequest = call.params(&params)?;
    uploads_allowed(&call)?;
    let name = req.name.trim().to_string();
    if name.is_empty() {
        return Err(RpcError::invalid_params("name is empty. Give the graphic a name people would call it, such as \"Storm warning lower third\".").with("field", "name"));
    }
    let mut draft = input::draft_of(&call, &req).await?;
    describe(&mut draft, &req)?;
    let id = gallery::slug(&name);
    let replace = req.replace;
    let d = draft.clone();
    let path = blocking("saving the graphic", move || gallery::store::write(&dir(), &gallery::slug(&d.manifest.name), &d, replace))
        .await?
        .map_err(|e| RpcError::invalid_params(format!("{e:#}. Nothing was saved.")).with("id", id.clone()))?;
    let mut entry = super::entry(&id).await?;
    gallery::preview::forget(&dir(), &id);
    let redrawn = if replace { manage::redraw(&call, &entry).await } else { Vec::new() };
    super::placed(&call, std::slice::from_mut(&mut entry)).await;
    let next = format!(
        "Look at it with preview_graphic {{\"id\": \"{id}\"}}, fix and save again with replace: true until it is right, then place_graphic {{\"id\": \"{id}\"}} and show_graphic {{\"id\": \"{id}\"}} to put it on air."
    );
    body(GallerySaved { item: entry.item, path: path.display().to_string(), redrawn, warnings: draft.warnings, next })
}

/// Saving writes files, which the station may have turned off.
pub(super) fn uploads_allowed(call: &Call) -> Result<(), RpcError> {
    if call.app.library.cfg().allow_upload {
        return Ok(());
    }
    Err(RpcError::not_in_state("this mixer does not take new files (media.allow_upload is off). Turn on Allow uploads in Settings, then save again.")
        .with_action(godwinmix_protocol::ErrorAction::open_setting("Allow uploads", "media.allow_upload")))
}

/// Put what the caller said about the graphic into its manifest.
fn describe(d: &mut Draft, req: &GallerySaveRequest) -> Result<(), RpcError> {
    let m = &mut d.manifest;
    m.name = req.name.trim().to_string();
    if let Some(kind) = req.kind.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        let asked = GalleryKind::parse(kind).ok_or_else(|| RpcError::invalid_params(format!("kind {kind:?} is not a gallery kind. Leave kind out; it is worked out from what you send.")).with("field", "kind"))?;
        let found = m.kind().unwrap_or(GalleryKind::Image);
        match (found, asked) {
            (a, b) if a == b => {}
            (GalleryKind::Image | GalleryKind::Clip, GalleryKind::Transition | GalleryKind::Effect) => m.kind = asked.as_str().into(),
            (GalleryKind::Html, GalleryKind::Ograf) => m.kind = asked.as_str().into(),
            _ => d.warnings.push(format!("kind {} was asked for and what was sent is a {}; it was saved as a {}", asked.as_str(), found.as_str(), found.as_str())),
        }
    }
    if let Some(zone) = req.zone.as_deref().map(str::trim).filter(|z| !z.is_empty()) {
        let z = Zone::parse(zone).ok_or_else(|| RpcError::invalid_params(format!("zone {zone:?} is not a zone. Use full, lower-third, bug, top, bottom, center or overlay, or leave it out.")).with("field", "zone"))?;
        m.zone = Some(z.as_str().into());
    }
    let tags = words(req.tags.as_ref());
    if !tags.is_empty() {
        m.tags = tags;
    }
    if let Some(desc) = req.description.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        m.description = desc.to_string();
    }
    m.moves = req.moves.or(m.moves);
    m.transparent = req.transparent.or(m.transparent);
    if let Some(values) = &req.values {
        m.values.extend(to_toml_table(values));
    }
    m.made_by = req.made_by.clone().unwrap_or_default();
    m.origin = Some(if m.origin.as_deref() == Some("uploaded") { "uploaded" } else { "agent" }.into());
    m.saved = now();
    Ok(())
}
