//! A picture of an item, drawn when somebody asks and kept until the item
//! changes.
//!
//! The picture is the item as it would land on a 16 by 9 canvas, in its
//! zone, over a checkerboard where it is transparent: what an operator sees
//! on a card and what a model reads to check its own work. It is drawn by
//! the same code that draws the item on air (the template renderer, the
//! picture decoder, the text and ticker renderers), so a preview that looks
//! right is right.
//!
//! ```text
//!   .previews/<id>__<width>_<backdrop>_<stamp>_<how>.jpg   one per ask, the
//!   newest kept; a changed item has a new stamp and draws again
//! ```

mod clip;
mod draw;
mod rgba;

use super::entry::Entry;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub use rgba::Backdrop;

/// A finished preview.
#[derive(Debug)]
pub struct Still {
    pub jpeg: Vec<u8>,
    pub width: u32,
    pub height: u32,
    /// `drawn`, `poster`, `frame` or `card`.
    pub from: &'static str,
}

/// The widths a preview may be.
pub const MIN_WIDTH: u32 = 64;
pub const MAX_WIDTH: u32 = 1920;

/// The preview of `entry` at `width`, from the cache when it is there.
/// `values` tries field values without saving them.
pub fn preview(gallery: &Path, entry: &Entry, width: u32, backdrop: Backdrop, values: &serde_json::Map<String, serde_json::Value>) -> Result<Still> {
    let width = width.clamp(MIN_WIDTH, MAX_WIDTH) & !1;
    let height = (width * 9 / 16) & !1;
    let stamp = entry.stamp() ^ hash(&serde_json::to_string(values).unwrap_or_default());
    let dir = gallery.join(".previews");
    let prefix = format!("{}__{width}_{}_{stamp:x}_", entry.item.id, backdrop.key());
    if let Some((path, from)) = cached(&dir, &prefix) {
        if let Ok(jpeg) = std::fs::read(&path) {
            return Ok(Still { jpeg, width, height, from });
        }
    }
    let (layers, from) = draw::layers(gallery, entry, (width, height), values)?;
    let mut img = backdrop.fill(width, height);
    for layer in &layers {
        rgba::blend(&mut img, layer);
    }
    let jpeg = crate::snapshot::encode_jpeg(&img).context("encoding the preview")?;
    store(&dir, &entry.item.id, &format!("{width}_{}_", backdrop.key()), &format!("{prefix}{from}.jpg"), &jpeg);
    Ok(Still { jpeg, width, height, from })
}

fn hash(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    h.finish()
}

fn cached(dir: &Path, prefix: &str) -> Option<(PathBuf, &'static str)> {
    let entries = std::fs::read_dir(dir).ok()?;
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if let Some(rest) = name.strip_prefix(prefix) {
            let from = ["drawn", "poster", "frame", "card"].into_iter().find(|f| rest == format!("{f}.jpg"))?;
            return Some((e.path(), from));
        }
    }
    None
}

/// Keep the new picture and drop older ones of the same item, size and
/// backdrop, so the cache holds one per ask and never grows past that.
fn store(dir: &Path, id: &str, shape: &str, name: &str, jpeg: &[u8]) {
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    let old = format!("{id}__{shape}");
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.flatten() {
            let n = e.file_name().to_string_lossy().to_string();
            if n.starts_with(&old) && n != name {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let _ = std::fs::write(dir.join(name), jpeg);
}

/// Forget every preview of `id`.
pub fn forget(gallery: &Path, id: &str) {
    let dir = gallery.join(".previews");
    let Ok(entries) = std::fs::read_dir(&dir) else { return };
    // Two underscores, which no id has, so `bar` does not take `bar-2`'s.
    let prefix = format!("{id}__");
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}
