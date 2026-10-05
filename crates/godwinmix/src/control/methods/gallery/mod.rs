//! `gallery.*`: the Graphics gallery, for the page, the CLI and an agent
//! alike.
//!
//! The tool names are plain on purpose (`save_graphic`, `list_graphics`,
//! `preview_graphic`, `place_graphic`, `show_graphic`) and the inputs are
//! forgiving, because a small free model calls these as often as a large
//! one does. Every refusal says what to call next.
//!
//! Everything that reads a disk or draws a picture runs on a blocking
//! thread, and cards are drawn two at a time (with one more lane for a
//! preview asked for by name), so a page opening a gallery of fifty items
//! never takes a core from the encoder.

mod files;
mod list;
mod place;
mod preview;
mod save;
mod show;

use crate::control::call::Call;
use godwinmix_core::gallery::{self, Entry};
use godwinmix_protocol::error::RpcError;
use godwinmix_protocol::method::Registry;
use std::path::PathBuf;

pub use files::{download_export, import_upload, serve_file};
pub use preview::preview_jpeg;

pub fn register(reg: &mut Registry<Call>) {
    list::register(reg);
    save::register(reg);
    preview::register(reg);
    place::register(reg);
    show::register(reg);
    files::register(reg);
}

/// The gallery's folder.
pub(crate) fn dir() -> PathBuf {
    gallery::dir()
}

/// Run blocking work off the async threads.
pub(crate) async fn blocking<T: Send + 'static>(what: &str, f: impl FnOnce() -> T + Send + 'static) -> Result<T, RpcError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| RpcError::internal(format!("{what}: {e}")))
}

/// One item by id or name, or the error that lists what there is.
pub(crate) async fn entry(id: &str) -> Result<Entry, RpcError> {
    let id = id.trim().to_string();
    if id.is_empty() {
        return Err(RpcError::invalid_params("which graphic? Give `id`, an id from list_graphics, such as \"news-lower-third\".").with("field", "id"));
    }
    let wanted = id.clone();
    blocking("reading the gallery", move || gallery::store::find(&dir(), &wanted)).await?.map_err(|e| missing(&id, e))
}

/// The refusal for an id the gallery does not have, with the ids it does.
pub(crate) fn missing(id: &str, e: anyhow::Error) -> RpcError {
    let (all, _) = gallery::store::list(&dir());
    let ids: Vec<String> = all.iter().map(|e| e.item.id.clone()).collect();
    RpcError::not_found("gallery item", id, &ids)
        .with("detail", e.to_string())
        .with("next", "Call list_graphics (gallery.list) with a few words of what you want, and use an id from it.")
}

/// The sources on this mixer drawing each item, by the item's address.
pub(crate) async fn placed(call: &Call, entries: &mut [Entry]) {
    let Ok(configs) = call.app.mixer.configs().await else { return };
    for e in entries.iter_mut() {
        // An HTML graphic's source reads it from the gallery's own route.
        let own = e.item.uri.clone().unwrap_or_else(|| format!("/api/v1/gallery/{}/files/", e.item.id));
        e.item.placed = configs
            .sources
            .iter()
            .filter(|s| same_uri(&own, &s.uri) || (e.item.uri.is_none() && s.uri.contains(&own)))
            .map(|s| s.id.clone())
            .collect();
    }
}

#[cfg(test)]
mod tests;

fn same_uri(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().trim_start_matches(r"\\?\").replace('\\', "/").to_ascii_lowercase();
    norm(a) == norm(b)
}

/// Words from a list or from one string with commas.
pub(crate) fn words(v: Option<&serde_json::Value>) -> Vec<String> {
    let raw: Vec<String> = match v {
        Some(serde_json::Value::Array(a)) => a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(),
        Some(serde_json::Value::String(s)) => s.split([',', ';']).map(str::to_string).collect(),
        _ => Vec::new(),
    };
    raw.into_iter().map(|w| w.trim().to_string()).filter(|w| !w.is_empty()).collect()
}
