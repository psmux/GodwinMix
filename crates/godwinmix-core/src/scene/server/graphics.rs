//! Graphics: an OGraf template placed on a scene like anything else.
//!
//! A graphic is not a new kind of thing on the canvas. It is a `graphic`
//! provide on a plugin, pointing at an OGraf `graphic.ograf.json`, rendered by
//! a browser page the graphics host serves, and reaching the compositor as an
//! ordinary source instance on an ordinary slot. Nothing in `mixer.rs` knows
//! what a graphic is, which is the point: the slot pool already draws sources,
//! and a graphic is one.
//!
//! ```text
//!   item { graphic: "ograf/lower-third", params: {name: "{{speaker}}"} }
//!        |
//!        |  values(): the OGraf schema's defaults, the item's own params,
//!        |            and {{name}} resolved against the collection's params
//!        v
//!   source "graphic-lower-third-a3f91c"  ->  browser/source on the host's page
//!        |
//!        v
//!   Placement, like every other item
//! ```
//!
//! What is here is the document half, and it is pure: reading the OGraf
//! manifest a plugin declares, working out what a graphic item is actually
//! rendered with, and filling fields by name. Starting the source and pushing
//! `load` and `playAction` at it is the RPC layer's job, because that is where
//! the mixer is.
//!
//! Fields are addressed by name and never by position (`scene.apply_graphic
//! {graphic, values}`, 11 section 6). A name that is not in the schema is
//! refused with the names that would have worked, because the caller is often
//! a model and a silent no-op is the one answer it cannot learn from.

use std::collections::BTreeMap;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::scene::document::{Collection, Content, Item};
use crate::scene::id::Id;

/// The prefix on every source instance a graphic resolves to, so an operator
/// reading `source.list` can tell at a glance what put it there.
pub const SOURCE_PREFIX: &str = "graphic-";

/// The OGraf manifest, in the subset this host reads.
///
/// Everything else the file carries is kept in `rest` and passed on: OGraf is
/// an EBU specification that will grow, and a key this build has not heard of
/// is a key a newer client may want. Dropping it here would make the core the
/// thing that has to be upgraded first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Ograf {
    /// The graphic's own id, as the OGraf file gives it.
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// The module the web component is in, relative to the manifest.
    #[serde(default)]
    pub main: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// How many steps `playAction` walks through. One means in and out.
    #[serde(default = "one", rename = "stepCount")]
    pub step_count: u32,
    #[serde(default = "yes", rename = "supportsRealTime")]
    pub supports_real_time: bool,
    #[serde(default, rename = "supportsNonRealTime")]
    pub supports_non_real_time: bool,
    /// The JSON Schema of the graphic's own data. What the inspector renders
    /// and what `scene.apply_graphic` fills by name.
    #[serde(default = "empty_schema")]
    pub schema: Value,
    /// Every other key in the file, carried through untouched.
    #[serde(flatten, default, skip_serializing_if = "Map::is_empty")]
    #[schemars(skip)]
    pub rest: Map<String, Value>,
}

fn one() -> u32 {
    1
}

fn yes() -> bool {
    true
}

fn empty_schema() -> Value {
    serde_json::json!({ "type": "object", "properties": {} })
}

/// One graphic this core can place, as the catalogue has it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GraphicType {
    /// The plugin qualified id an item's `content.graphic` names,
    /// `ograf/lower-third`.
    pub type_id: String,
    pub plugin: String,
    pub provide: String,
    /// The OGraf manifest's path inside the plugin, so a client can fetch it.
    pub manifest: String,
    pub ograf: Ograf,
    /// The `[provides.designer]` block, when the plugin wrote one: the icon
    /// for the add gallery, the UI schema, the default frame and the gizmos.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub designer: Option<Value>,
}

impl GraphicType {
    /// What a picker puts under the tile.
    pub fn title(&self) -> &str {
        if self.ograf.name.is_empty() {
            &self.provide
        } else {
            &self.ograf.name
        }
    }

    /// The frame the designer should place it in, when the plugin said.
    pub fn default_frame(&self) -> Option<(f64, f64)> {
        let frame = self.designer.as_ref()?.get("default_frame")?;
        let frame = frame.get("frame").unwrap_or(frame);
        Some((frame.get("w")?.as_f64()?, frame.get("h")?.as_f64()?))
    }
}

// ---------------------------------------------------------------------------
// The catalogue
// ---------------------------------------------------------------------------

/// Every graphic on every live plugin.
///
/// Read off disk each time rather than cached: the list changes when a plugin
/// is installed or reloaded, it is asked for when somebody opens a picker, and
/// a stale gallery is a worse bug than a file read.
pub fn catalogue() -> Vec<GraphicType> {
    crate::plugin::loader::provides_of_kind("graphic")
        .into_iter()
        .filter_map(|type_id| read(&type_id).ok())
        .collect()
}

/// One graphic, by its plugin qualified id.
pub fn find(type_id: &str) -> Result<GraphicType> {
    read(type_id).with_context(|| {
        let known = names();
        match known.is_empty() {
            true => format!(
                "there is no graphic {type_id:?}, and this core has none installed. \
                 Install one with `gmx plugin add <name>`, or write one with \
                 `gmx plugin new --kind graphic <name>`."
            ),
            false => format!(
                "there is no graphic {type_id:?}. This core has: {}",
                known.join(", ")
            ),
        }
    })
}

/// The ids of every graphic, for an error that lists them.
pub fn names() -> Vec<String> {
    crate::plugin::loader::provides_of_kind("graphic")
}

/// The OGraf schema a graphic declares: what `scene.item.schema` answers with
/// for a graphic item.
pub fn schema(type_id: &str) -> Result<Value> {
    Ok(find(type_id)?.ograf.schema)
}

fn read(type_id: &str) -> Result<GraphicType> {
    let (plugin_name, provide_id) = type_id
        .split_once('/')
        .with_context(|| format!("{type_id:?} is not a plugin qualified id. Write it as <plugin>/<graphic>."))?;
    let plugin = crate::plugin::loader::get(plugin_name)
        .with_context(|| format!("the plugin {plugin_name:?} is not installed"))?;
    let decl = plugin
        .manifest
        .provide(provide_id)
        .with_context(|| format!("the plugin {plugin_name:?} provides no {provide_id:?}"))?;
    let relative = decl.graphic.as_ref().with_context(|| {
        format!("{type_id} is a {} provide, not a graphic", decl.kind)
    })?;
    let at = plugin.root.join(relative);
    let text = std::fs::read_to_string(&at)
        .with_context(|| format!("reading the OGraf manifest at {}", at.display()))?;
    let ograf: Ograf = serde_json::from_str(&text)
        .with_context(|| format!("reading the OGraf manifest at {}", at.display()))?;
    Ok(GraphicType {
        type_id: type_id.to_string(),
        plugin: plugin_name.to_string(),
        provide: provide_id.to_string(),
        manifest: relative.clone(),
        ograf,
        designer: decl.designer.as_ref().and_then(|d| serde_json::to_value(d).ok()),
    })
}

// ---------------------------------------------------------------------------
// From an item to a source
// ---------------------------------------------------------------------------

/// The source instance id a graphic item resolves to.
///
/// The graphic's provide id and eight hex of the item's own id: legible in
/// `source.list`, stable across a rename (an item's name is not in it), and
/// unique per placement, which is what lets the same template be on the canvas
/// twice with different words in it.
///
/// The eight hex come off the *end* of the id and not the start. An item id is
/// a UUIDv7 and the leading bytes are the millisecond it was minted, so two
/// items added in the same second share their first six characters; the tail
/// is the random field. A test caught this, which is why it says so.
pub fn source_id(type_id: &str, item: &Id) -> String {
    let provide = type_id.rsplit('/').next().unwrap_or(type_id);
    let slug: String = provide
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' })
        .collect();
    let digest = item.to_string().replace('-', "");
    let tail = &digest[digest.len().saturating_sub(8)..];
    format!("{SOURCE_PREFIX}{}-{}", slug.trim_matches('-'), tail)
}

/// The page the graphics host serves this placement at.
///
/// One page per instance, never one page with several graphics on it: a
/// template that throws takes its own page down and not the other three.
pub fn page_url(base: &str, type_id: &str, item: &Id) -> String {
    format!(
        "{}/graphic/{}?instance={}",
        base.trim_end_matches('/'),
        type_id.trim_matches('/'),
        source_id(type_id, item)
    )
}

/// Every graphic item in the document, with the source each resolves to.
pub fn placements(doc: &Collection) -> Vec<(Id, String, String)> {
    let mut out = Vec::new();
    for scene in &doc.scenes {
        for item in scene.walk() {
            if let Content::Graphic { graphic, .. } = &item.content {
                out.push((item.id, graphic.clone(), source_id(graphic, &item.id)));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Values
// ---------------------------------------------------------------------------

/// What a graphic item is actually rendered with.
///
/// Three layers, lowest first: the OGraf schema's own defaults, the item's
/// `params`, and then `{{name}}` in any string resolved against the
/// collection's parameters. That last layer is what makes
/// `scene.params.set {speaker: "Ada"}` change every lower third at once
/// without touching an item.
pub fn values(doc: &Collection, item: &Item, ograf: Option<&Ograf>) -> Value {
    let Content::Graphic { params, .. } = &item.content else { return Value::Null };
    let mut out = Map::new();
    if let Some(ograf) = ograf {
        for (key, property) in properties(&ograf.schema) {
            if let Some(default) = property.get("default") {
                out.insert(key, default.clone());
            }
        }
    }
    if let Some(object) = params.as_object() {
        for (key, value) in object {
            out.insert(key.clone(), value.clone());
        }
    }
    let bindings = param_values(doc);
    Value::Object(out.into_iter().map(|(k, v)| (k, bind(&v, &bindings))).collect())
}

/// The collection's parameters as plain values: each property's `default`.
///
/// `scene.params.set` writes into the schema's `default`, which is what makes
/// the parameters readable without their values (11 section 2) and is why this
/// reads them from there rather than from a separate table.
pub fn param_values(doc: &Collection) -> BTreeMap<String, Value> {
    properties(&doc.params)
        .into_iter()
        .filter_map(|(key, property)| Some((key, property.get("default")?.clone())))
        .collect()
}

/// The `properties` object of a JSON Schema, as pairs.
fn properties(schema: &Value) -> Vec<(String, Value)> {
    schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        .unwrap_or_default()
}

/// Resolve `{{name}}` through a value, strings only.
///
/// A whole string that is one binding takes the parameter's own type, so a
/// number stays a number and a boolean stays a boolean. A binding nobody has
/// filled in is left as it stands, so a half filled graphic shows what is
/// missing rather than a blank.
fn bind(value: &Value, params: &BTreeMap<String, Value>) -> Value {
    match value {
        Value::String(s) => {
            let trimmed = s.trim();
            if let Some(inner) = trimmed.strip_prefix("{{").and_then(|r| r.strip_suffix("}}")) {
                if let Some(found) = params.get(inner.trim()) {
                    return found.clone();
                }
            }
            Value::String(bind_text(s, params))
        }
        Value::Array(a) => Value::Array(a.iter().map(|v| bind(v, params)).collect()),
        Value::Object(o) => {
            Value::Object(o.iter().map(|(k, v)| (k.clone(), bind(v, params))).collect())
        }
        other => other.clone(),
    }
}

fn bind_text(text: &str, params: &BTreeMap<String, Value>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(end) = rest[start..].find("}}") else { break };
        out.push_str(&rest[..start]);
        let key = rest[start + 2..start + end].trim();
        match params.get(key) {
            Some(Value::String(s)) => out.push_str(s),
            Some(other) => out.push_str(&other.to_string()),
            None => out.push_str(&rest[start..start + end + 2]),
        }
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

// ---------------------------------------------------------------------------
// Filling fields by name
// ---------------------------------------------------------------------------

/// What one `scene.apply_graphic` did.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Applied {
    /// The graphic that was filled.
    pub graphic: String,
    /// The items it was written onto, by id.
    pub items: Vec<Id>,
    /// The item names, so the answer reads without a second call.
    pub names: Vec<String>,
    /// The values as the graphic will render them: defaults, the item's own
    /// params, and the collection's parameters resolved.
    pub values: Value,
}

/// Write values into every graphic item of this type, by field name.
///
/// `which` narrows it to one item, by name or id, for a scene with two lower
/// thirds in it. With no `which` every placement of that graphic is filled,
/// which is what "put the speaker's name up" means when there is one.
pub fn apply(
    doc: &mut Collection,
    graphic: &str,
    fields: &Map<String, Value>,
    which: Option<&str>,
) -> Result<Vec<Id>> {
    let known = schema(graphic).ok();
    if let Some(schema) = &known {
        let allowed: Vec<String> = properties(schema).into_iter().map(|(k, _)| k).collect();
        // An unfillable name is refused rather than written and ignored: the
        // caller is often a model, and a silent no-op teaches it nothing.
        if let Some(unknown) = fields.keys().find(|k| !allowed.contains(k)) {
            bail!(
                "the graphic {graphic} has no field {unknown:?}. It takes: {}. \
                 Read them with scene.item.schema.",
                if allowed.is_empty() { "nothing".into() } else { allowed.join(", ") }
            );
        }
    }
    let mut touched = Vec::new();
    for scene in &mut doc.scenes {
        write_into(&mut scene.items, graphic, fields, which, &mut touched);
    }
    if touched.is_empty() {
        let placed: Vec<String> = doc
            .scenes
            .iter()
            .flat_map(|s| s.walk())
            .filter_map(|i| match &i.content {
                Content::Graphic { graphic, .. } => Some(graphic.clone()),
                _ => None,
            })
            .collect();
        match which {
            Some(which) => bail!(
                "no item {which:?} in this collection shows the graphic {graphic}. \
                 Add one with scene.item.add {{content: {{graphic: \"{graphic}\"}}}}."
            ),
            None => bail!(
                "no item in this collection shows the graphic {graphic}. \
                 The graphics on the canvas are: {}. Add one with scene.item.add \
                 {{content: {{graphic: \"{graphic}\"}}}}.",
                if placed.is_empty() { "none".into() } else { placed.join(", ") }
            ),
        }
    }
    Ok(touched)
}

fn write_into(
    items: &mut [Item],
    graphic: &str,
    fields: &Map<String, Value>,
    which: Option<&str>,
    touched: &mut Vec<Id>,
) {
    for item in items.iter_mut() {
        let named = match which {
            None => true,
            Some(w) => {
                item.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(w.trim()))
                    || item.id.to_string() == w.trim()
            }
        };
        if let Content::Graphic { graphic: kind, params } = &mut item.content {
            if kind == graphic && named {
                let mut object = params.as_object().cloned().unwrap_or_default();
                for (key, value) in fields {
                    object.insert(key.clone(), value.clone());
                }
                *params = Value::Object(object);
                touched.push(item.id);
            }
        }
        if let Content::Children { children } = &mut item.content {
            write_into(children, graphic, fields, which, touched);
        }
    }
}

/// The answer `scene.apply_graphic` gives, built after the edit landed.
pub fn applied(doc: &Collection, graphic: &str, items: &[Id]) -> Applied {
    let ograf = find(graphic).ok().map(|g| g.ograf);
    let mut names = Vec::new();
    let mut values = Value::Null;
    for scene in &doc.scenes {
        for item in scene.walk() {
            if !items.contains(&item.id) {
                continue;
            }
            names.push(item.name.clone().unwrap_or_else(|| item.id.to_string()));
            if values.is_null() {
                values = self::values(doc, item, ograf.as_ref());
            }
        }
    }
    Applied { graphic: graphic.to_string(), items: items.to_vec(), names, values }
}

#[cfg(test)]
mod tests;
