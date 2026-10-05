//! `gallery.preview`: a picture of an item for a model to look at, and the
//! same picture as bytes for the page's cards.
//!
//! Drawn on a blocking thread, three at a time at most, and kept on disk
//! until the item changes, so a gallery of fifty cards opened on a phone
//! costs fifty small renders once and nothing after.

use super::{dir, entry};
use crate::control::call::Call;
use crate::control::methods::{body, handler};
use base64::Engine;
use godwinmix_core::gallery::preview::{self, Backdrop, Still};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::gallery::*;
use godwinmix_protocol::method::{schema_of, MethodDef, Registry, Tier};
use godwinmix_protocol::scope::Scope;
use serde_json::{Map, Value};
use std::sync::LazyLock;
use tokio::sync::Semaphore;

/// Cards are drawn two at a time. A preview somebody asked for by name (an
/// agent checking its work, a person editing words) has a lane of its own,
/// so it never waits behind a page full of cards.
static CARDS: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(2));
static ASKED: LazyLock<Semaphore> = LazyLock::new(|| Semaphore::new(1));

pub fn register(reg: &mut Registry<Call>) {
    reg.register(
        MethodDef::new("gallery.preview", Scope::Read, "A picture of a gallery item as it would land on the canvas, transparent parts over a checkerboard, drawn on demand.", handler(preview_call))
            .params(schema_of::<GalleryPreviewRequest>)
            .result(schema_of::<GalleryPreview>)
            .tool(
                "preview_graphic",
                Tier::Search,
                "Look at a gallery graphic as a picture: it is drawn where it lands on a 16:9 screen, over a \
                 grey checkerboard where it is transparent. Use it after save_graphic to check your own work: \
                 are the words inside their panel, is anything cut off, is it readable. Optional `values` tries \
                 field words without saving, `width` (default 960), `background` (checker, black, white, #rrggbb).",
            ),
    );
}

async fn preview_call(call: Call, params: Value) -> Result<Value, RpcError> {
    let req: GalleryPreviewRequest = call.params(&params)?;
    let backdrop = Backdrop::parse(req.background.as_deref()).map_err(|e| RpcError::invalid_params(e.to_string()).with("field", "background"))?;
    let values = req.values.clone().unwrap_or_default();
    let (still, item) = draw(&req.id, req.width.unwrap_or(960), backdrop, values, &ASKED).await?;
    let caption = format!(
        "{} ({}, {}{}), {} on a {}x{} preview",
        item.name,
        item.kind.as_str(),
        item.zone.as_str(),
        if item.transparent { ", transparent" } else { "" },
        match still.from {
            "card" => "a placeholder card: this kind is drawn by a browser; place it and look with preview_frame",
            "poster" => "its own preview picture",
            "frame" => "a frame from the middle",
            _ => "drawn by the mixer",
        },
        still.width,
        still.height
    );
    body(GalleryPreview {
        id: item.id,
        width: still.width,
        height: still.height,
        image: base64::engine::general_purpose::STANDARD.encode(&still.jpeg),
        encoding: "base64".into(),
        format: "jpeg".into(),
        from: still.from.into(),
        caption,
    })
}

/// Draw `id`, waiting for a turn.
async fn draw(id: &str, width: u32, backdrop: Backdrop, values: Map<String, Value>, lane: &Semaphore) -> Result<(Still, GalleryItem), RpcError> {
    let e = entry(id).await?;
    let item = e.item.clone();
    let _turn = lane.acquire().await.map_err(|_| RpcError::internal("the preview queue closed"))?;
    let still = super::blocking("drawing the preview", move || preview::preview(&dir(), &e, width, backdrop, &values))
        .await?
        .map_err(|err| {
            RpcError::invalid_params(format!("{} could not be drawn: {err:#}. Fix it and save it again with replace: true.", item.id))
                .with("id", item.id.clone())
        })?;
    Ok((still, item))
}

/// The JPEG alone, for `GET /api/v1/gallery/{id}/preview.jpg`.
pub async fn preview_jpeg(id: &str, width: Option<u32>, background: Option<&str>) -> Result<Vec<u8>, RpcError> {
    let backdrop = Backdrop::parse(background).map_err(|e| RpcError::invalid_params(e.to_string()))?;
    let (still, _) = draw(id, width.unwrap_or(480), backdrop, Map::new(), &CARDS).await?;
    Ok(still.jpeg)
}
