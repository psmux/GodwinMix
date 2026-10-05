//! `graphic.toml`: what an item is, read and written.
//!
//! Every key but `name` and `kind` may be left out, and keys this build does
//! not know are kept in `rest` and written back, so an item saved by a newer
//! mixer, or carrying a table that belongs to another feature
//! (`[transition]`, `[effect]`), survives an edit here untouched.

use anyhow::{bail, Context, Result};
use godwinmix_protocol::gallery::{GalleryKind, Origin, Zone};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub zone: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moves: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transparent: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub saved: String,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub values: toml::Table,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<SourceSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set: Option<SetToml>,
    /// Everything else, kept as it was.
    #[serde(flatten)]
    pub rest: toml::Table,
}

/// `[source]`: a ticker or a text, as `source.add` takes it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SourceSpec {
    pub uri: String,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub params: toml::Table,
}

/// `[set]`: a virtual set's pictures and its layout's settings.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SetToml {
    pub background: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub foreground: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "toml::Table::is_empty")]
    pub settings: toml::Table,
}

impl Manifest {
    /// Read `text`, naming the file in the error.
    pub fn parse(text: &str, at: &str) -> Result<Manifest> {
        let m: Manifest = toml::from_str(text).with_context(|| format!("{at} is not a graphic.toml this mixer can read"))?;
        if m.name.trim().is_empty() {
            bail!("{at} has no name. Add a line such as name = \"My lower third\"");
        }
        if m.kind().is_none() {
            let kinds: Vec<&str> = GalleryKind::ALL.iter().map(|k| k.as_str()).collect();
            bail!("{at} says kind = {:?}, which is not one of {}", m.kind, kinds.join(", "));
        }
        Ok(m)
    }

    /// Read the manifest in folder `dir`.
    pub fn read(dir: &Path) -> Result<Manifest> {
        let path = dir.join(super::MANIFEST);
        let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text, &path.display().to_string())
    }

    pub fn to_toml(&self) -> Result<String> {
        toml::to_string_pretty(self).context("writing graphic.toml")
    }

    pub fn kind(&self) -> Option<GalleryKind> {
        GalleryKind::parse(&self.kind)
    }

    pub fn zone(&self) -> Option<Zone> {
        self.zone.as_deref().and_then(Zone::parse)
    }

    pub fn origin(&self) -> Origin {
        match self.origin.as_deref().map(str::trim) {
            Some("shipped") => Origin::Shipped,
            Some("uploaded") | Some("imported") => Origin::Uploaded,
            _ => Origin::Agent,
        }
    }

    /// The values as JSON, for the wire and for `params.fields`.
    pub fn values_json(&self) -> serde_json::Map<String, serde_json::Value> {
        to_json_map(&self.values)
    }
}

/// A TOML table as a JSON object.
pub fn to_json_map(t: &toml::Table) -> serde_json::Map<String, serde_json::Value> {
    match serde_json::to_value(t) {
        Ok(serde_json::Value::Object(m)) => m,
        _ => serde_json::Map::new(),
    }
}

/// A JSON object as a TOML table. `null` has no TOML form and is dropped,
/// which is what a caller means by it: take this one away.
pub fn to_toml_table(m: &serde_json::Map<String, serde_json::Value>) -> toml::Table {
    let mut out = toml::Table::new();
    for (k, v) in m {
        if let Some(v) = to_toml_value(v) {
            out.insert(k.clone(), v);
        }
    }
    out
}

fn to_toml_value(v: &serde_json::Value) -> Option<toml::Value> {
    use serde_json::Value as J;
    Some(match v {
        J::Null => return None,
        J::Bool(b) => toml::Value::Boolean(*b),
        J::Number(n) => n.as_i64().map(toml::Value::Integer).or_else(|| n.as_f64().map(toml::Value::Float))?,
        J::String(s) => toml::Value::String(s.clone()),
        J::Array(a) => toml::Value::Array(a.iter().filter_map(to_toml_value).collect()),
        J::Object(m) => toml::Value::Table(to_toml_table(m)),
    })
}
