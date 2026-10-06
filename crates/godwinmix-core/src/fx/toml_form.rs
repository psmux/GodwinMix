//! An fx item on disk: the gallery's `graphic.toml`, with `kind` set to
//! `transition` or `effect` and the fx settings in a table of that name.
//!
//! ```toml
//! name = "Light leak"
//! kind = "transition"
//! file = "light-leak.webm"
//! tags = ["light leak", "overlay"]
//! origin = "shipped"
//!
//! [transition]
//! look = "overlay"
//! blend = "screen"
//! duration_ms = 2000
//! cut_at_measured_ms = 1000
//! effect = true
//! ```
//!
//! The gallery lists, previews, exports and deletes these like any of its
//! items and passes the table through; this module is the only reader of
//! the table. Keys it does not know, in the table or outside it, are kept.

use anyhow::{bail, Context, Result};
use godwinmix_protocol::fx::{FxBlend, FxKind, FxManifest};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Item {
    name: String,
    kind: String,
    file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    origin: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    transition: Option<Table>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect: Option<Table>,
    #[serde(flatten)]
    other: toml::Table,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Table {
    look: FxKind,
    #[serde(default)]
    blend: FxBlend,
    duration_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cut_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cut_at_measured_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coverage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    softness: Option<f64>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    invert: bool,
    #[serde(default)]
    transition: bool,
    #[serde(default)]
    effect: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    licence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<String>,
    #[serde(flatten)]
    other: toml::Table,
}

/// Read one `graphic.toml`. `Ok(None)` for a gallery item that is not a
/// transition or an effect, which is the gallery's business and not ours.
pub fn parse(id: &str, text: &str) -> Result<Option<FxManifest>> {
    let item: Item = toml::from_str(text).context("graphic.toml does not read; see docs/reference/fx.md")?;
    let t = match item.kind.as_str() {
        "transition" => item.transition.clone(),
        "effect" => item.effect.clone(),
        _ => return Ok(None),
    };
    let Some(t) = t else { bail!("a {} item needs a [{}] table; see docs/reference/fx.md", item.kind, item.kind) };
    Ok(Some(FxManifest {
        name: id.to_string(),
        title: item.name,
        kind: t.look,
        blend: t.blend,
        file: item.file,
        duration_ms: t.duration_ms,
        cut_at_ms: t.cut_at_ms,
        cut_at_measured_ms: t.cut_at_measured_ms,
        coverage: t.coverage,
        softness: t.softness,
        invert: t.invert,
        transition: t.transition || item.kind == "transition",
        effect: t.effect || item.kind == "effect",
        licence: t.licence,
        source: t.source,
    }))
}

/// Write `m` as a `graphic.toml`, keeping what `old` had that is not ours.
pub fn render(m: &FxManifest, old: Option<&str>, origin: &str) -> Result<String> {
    let kept: Option<Item> = old.and_then(|t| toml::from_str(t).ok());
    let kind = if m.transition || !m.effect { "transition" } else { "effect" };
    let kept_table = kept.as_ref().and_then(|k| k.transition.clone().or_else(|| k.effect.clone())).map(|t| t.other).unwrap_or_default();
    let table = Table {
        look: m.kind,
        blend: m.blend,
        duration_ms: m.duration_ms,
        cut_at_ms: m.cut_at_ms,
        cut_at_measured_ms: m.cut_at_measured_ms,
        coverage: m.coverage.map(|c| (c * 1000.0).round() / 1000.0),
        softness: m.softness,
        invert: m.invert,
        transition: m.transition,
        effect: m.effect,
        licence: m.licence.clone(),
        source: m.source.clone(),
        other: kept_table,
    };
    let look = format!("{:?}", m.kind).to_lowercase();
    let item = Item {
        name: if m.title.is_empty() { m.name.clone() } else { m.title.clone() },
        kind: kind.to_string(),
        file: m.file.clone(),
        description: kept.as_ref().and_then(|k| k.description.clone()),
        tags: kept.as_ref().map(|k| k.tags.clone()).filter(|t| !t.is_empty()).unwrap_or_else(|| vec![look, format!("{:?}", m.blend).to_lowercase()]),
        origin: Some(kept.as_ref().and_then(|k| k.origin.clone()).unwrap_or_else(|| origin.to_string())),
        transition: (kind == "transition").then(|| table.clone()),
        effect: (kind == "effect").then_some(table),
        other: kept.map(|k| k.other).unwrap_or_default(),
    };
    Ok(toml::to_string(&item)?)
}
