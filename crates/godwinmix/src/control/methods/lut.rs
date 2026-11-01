//! Looks: list, import and remove LUTs, save and remove presets, and preview
//! them on a camera. A filter uses one by name (`lut/filter` with `lut` or
//! `preset`), which `godwinmix_core::lut::library` looks up.
//!
//! Everything that reads or writes the library runs on a blocking worker: an
//! import can read a 65 point cube, and a preview grades a still per look.

use super::{body, handler};
use crate::control::call::Call;
use base64::Engine;
use godwinmix_core::plugin::filters::chroma::still::RgbImage;
use godwinmix_core::lut::{self, library, preview};
use godwinmix_protocol::error::{ErrorCode, RpcError};
use godwinmix_protocol::lut::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{json, Value};

#[path = "lut_tools.rs"]
mod tools;

pub fn register(reg: &mut Registry<Call>) {
    tools::register(reg);
}

/// Run `f` on a blocking worker and turn its error into a refusal that
/// carries the sentence and where to look next.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> anyhow::Result<T> + Send + 'static) -> Result<T, RpcError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| RpcError::internal(format!("the LUT worker stopped: {e}")))?
        .map_err(|e| RpcError::new(ErrorCode::InvalidParams, format!("{e:#}")).with("list", "lut.list").with("formats", json!(lut::FORMATS)))
}

async fn list(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutListRequest = call.params(&params)?;
    let dir = library::dir();
    let (luts, presets, errors) = blocking(move || {
        let (luts, mut errors) = library::list(&dir);
        let presets = library::presets(&dir).unwrap_or_else(|e| {
            errors.push(format!("{e:#}"));
            Vec::new()
        });
        Ok((luts, presets, errors))
    })
    .await?;
    let luts = luts.into_iter().filter(|l| req.group.as_deref().is_none_or(|g| g == l.group)).collect();
    body(LutList { luts, presets, errors, formats: lut::FORMATS.iter().map(|s| s.to_string()).collect() })
}

async fn import(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutImportRequest = call.params(&params)?;
    let media = call.app.library.dir().to_path_buf();
    let entry = blocking(move || {
        let path = godwinmix_core::fx::import::locate(&media, &req.path)?;
        library::import(&library::dir(), &path, req.name.as_deref(), req.title.as_deref())
    })
    .await?;
    body(LutImported { lut: entry })
}

async fn remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutNameRequest = call.params(&params)?;
    let name = req.name.clone();
    blocking(move || library::remove(&library::dir(), &req.name)).await?;
    body(json!({"removed": name}))
}

async fn preset_save(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutPreset = call.params(&params)?;
    let saved = blocking(move || library::save_preset(&library::dir(), req)).await?;
    body(saved)
}

async fn preset_remove(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutNameRequest = call.params(&params)?;
    let name = req.name.clone();
    blocking(move || library::remove_preset(&library::dir(), &req.name)).await?;
    body(json!({"removed": name}))
}

async fn preview_looks(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: LutPreviewRequest = call.params(&params)?;
    // A camera that has no picture yet still gets a picker full of looks:
    // the test card stands in rather than the ask being refused.
    let still = match &req.source {
        Some(s) => super::scenes::key_color::still_of(&call, s).await.ok(),
        None => None,
    };
    let shown = still.as_ref().and(req.source.clone());
    let out = blocking(move || render(&req, still)).await?;
    body(LutPreviews { source: shown, original: out.0, previews: out.1 })
}

fn render(req: &LutPreviewRequest, still: Option<RgbImage>) -> anyhow::Result<(String, Vec<LutPreview>)> {
    let width = req.width.unwrap_or(240);
    let base = match still {
        Some(s) => preview::fit(&s, width),
        None => preview::test_card(width),
    };
    let dir = library::dir();
    let names = match &req.luts {
        Some(n) => n.clone(),
        None => library::list(&dir).0.into_iter().map(|e| e.name).collect(),
    };
    let amount = req.amount.unwrap_or(1.0).clamp(0.0, 1.0);
    let mut jobs: Vec<(String, &str, toml::Table)> = names
        .into_iter()
        .map(|n| {
            let mut p = toml::Table::new();
            p.insert("lut".into(), n.clone().into());
            p.insert("amount".into(), amount.into());
            (n, "lut", p)
        })
        .collect();
    for n in &req.presets {
        let mut p = toml::Table::new();
        p.insert("preset".into(), n.clone().into());
        jobs.push((n.clone(), "preset", p));
    }
    let mut previews = Vec::with_capacity(jobs.len());
    for (name, kind, params) in jobs {
        let img = match godwinmix_core::plugin::filters::lut::table_for(&params, &dir)? {
            Some(t) => preview::graded(&base, &t),
            None => base.clone(),
        };
        previews.push(LutPreview { name, kind: kind.into(), image: data_url(&img)? });
    }
    Ok((data_url(&base)?, previews))
}

fn data_url(img: &RgbImage) -> anyhow::Result<String> {
    Ok(format!("data:image/jpeg;base64,{}", base64::engine::general_purpose::STANDARD.encode(preview::jpeg(img)?)))
}
