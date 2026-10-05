//! `gallery.export` and `gallery.import`: a look carried between mixers as
//! one zip, and files made elsewhere checked and taken in one by one.

#[path = "files_http.rs"]
mod http;

use super::{blocking, dir, words};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use godwinmix_core::gallery::{self, bundle, draft::Draft, draft::Refusal, edit::now};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::Value;
use std::path::PathBuf;

pub use http::{download_export, import_upload, serve_file};

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.export", Scope::Operate, "Write gallery items to one zip on the mixer, to carry a look to another mixer.", handler(export))
            .params(schema_of::<GalleryExportRequest>)
            .result(schema_of::<GalleryExported>)
            .not_idempotent()
            .tool("export_graphics", Tier::Search, "Pack gallery graphics into one zip file to share a look with another GodwinMix. `ids` picks which (default: every one not shipped). Answers the zip's path and a download URL."),
    );
    reg.register(
        MethodDef::new("gallery.import", Scope::Operate, "Take files into the gallery: a gallery zip, an SVG, an HTML page or folder, an OGraf package, a picture or a clip. Each is checked; refused ones say why and how to fix them.", handler(import))
            .params(schema_of::<GalleryImportRequest>)
            .result(schema_of::<GalleryImported>)
            .not_idempotent()
            .tool("import_graphics", Tier::Search, "Import graphic files into the gallery from a path on the mixer: a gallery zip, a folder of graphics, an SVG, an HTML page, a PNG or WebP, a WebM or MOV. Each file is checked; any refused comes back with the reason and the fix."),
    );
}

async fn export(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryExportRequest = if params.is_null() { Default::default() } else { call.params(&params)? };
    let ids = words(req.ids.as_ref());
    let (all, _) = blocking("reading the gallery", || gallery::store::list(&dir())).await?;
    let chosen: Vec<_> = match ids.is_empty() {
        true => all.into_iter().filter(|e| !e.read_only()).collect(),
        false => {
            let mut out = Vec::new();
            for id in &ids {
                out.push(super::entry(id).await?);
            }
            out
        }
    };
    if chosen.is_empty() {
        return Err(RpcError::not_in_state("there is nothing of your own in the gallery to export yet. Save a graphic first, or name shipped ones in ids."));
    }
    let exports = dir().join("exports");
    let stamp: String = now().chars().filter(|c| c.is_ascii_digit()).take(14).collect();
    let path = req.path.as_deref().map(str::trim).filter(|p| !p.is_empty()).map(PathBuf::from).unwrap_or_else(|| exports.join(format!("gallery-{stamp}.zip")));
    let ids: Vec<String> = chosen.iter().map(|e| e.item.id.clone()).collect();
    let dest = path.clone();
    let size = blocking("writing the zip", move || bundle::export(&chosen, &dest)).await?.map_err(|e| RpcError::internal(format!("{e:#}")))?;
    let url = match path.parent() == Some(exports.as_path()) {
        true => format!("/api/v1/gallery/exports/{}", path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()),
        false => String::new(),
    };
    body(GalleryExported { path: path.display().to_string(), size_bytes: size, ids, url })
}

async fn import(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryImportRequest = call.params(&params)?;
    super::save::uploads_allowed(&call)?;
    let found = match (&req.path, &req.data) {
        (Some(p), _) => {
            let path = call.app.library.resolve(p).unwrap_or_else(|_| PathBuf::from(p.trim()));
            blocking("reading the files", move || bundle::read_path(&path)).await?
        }
        (None, Some(data)) => {
            let (ext, bytes) = super::save::decode(data)?;
            let name = req.filename.clone().unwrap_or_else(|| format!("upload.{}", ext.unwrap_or("bin")));
            blocking("reading the files", move || bundle::read_bytes(&name, bytes)).await?
        }
        (None, None) => return Err(RpcError::invalid_params("import_graphics needs `path`, a file or folder on the mixer, or `data` with `filename`.")),
    };
    body(take_in(found, req.replace).await?)
}

/// Write every draft that was found, and answer what went in and what did
/// not, with why.
pub(crate) async fn take_in(found: Vec<(String, Result<Draft, Refusal>)>, replace: bool) -> Result<GalleryImported, RpcError> {
    blocking("importing", move || {
        let mut out = GalleryImported::default();
        for (file, drafted) in found {
            let d = match drafted {
                Ok(d) => d,
                Err(r) => {
                    out.refused.push(Refused { file, reason: r.reason, fix: r.fix });
                    continue;
                }
            };
            match write_one(d, &file, replace) {
                Ok(item) => out.added.push(item),
                Err(e) => out.refused.push(Refused { file, reason: format!("{e:#}"), fix: "Check the gallery folder can be written, then import it again.".into() }),
            }
        }
        out
    })
    .await
}

fn write_one(mut d: Draft, file: &str, replace: bool) -> anyhow::Result<GalleryItem> {
    let g = dir();
    if d.manifest.name.trim().is_empty() {
        d.manifest.name = gallery::draft::stem(file);
    }
    d.manifest.origin.get_or_insert_with(|| "uploaded".into());
    if d.manifest.origin.as_deref() == Some("shipped") {
        d.manifest.origin = Some("uploaded".into());
    }
    if d.manifest.saved.is_empty() {
        d.manifest.saved = now();
    }
    let id = match replace {
        true => gallery::slug(&d.manifest.name),
        false => gallery::store::free_id(&g, &d.manifest.name),
    };
    gallery::store::write(&g, &id, &d, replace)?;
    gallery::preview::forget(&g, &id);
    Ok(gallery::store::find(&g, &id)?.item)
}
