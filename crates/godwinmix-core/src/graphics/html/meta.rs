//! An HTML template's declaration: one JSON block in the page.
//!
//! ```html
//! <script type="application/json" id="gmx-template">
//! {"title": "Glass lower third", "category": "lower-third", "out_ms": 600,
//!  "fields": {"name": {"label": "Name", "default": "Ada Lovelace"},
//!             "accent": {"type": "color", "default": "#c8102e"}}}
//! </script>
//! ```
//!
//! JSON rather than markup because a model writes it right first time, and
//! a script of type `application/json` is ignored by the browser.

use super::problem::{error, Problems};
use crate::graphics::template::{is_name, FieldType, TemplateField};
use serde_json::Value;

/// The id the block is found by.
pub const BLOCK_ID: &str = "gmx-template";
/// The longest way out a template may declare. Ten seconds is the ceiling
/// every transition has.
pub const MAX_OUT_MS: u64 = 10_000;

/// What the block says.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Meta {
    pub title: Option<String>,
    pub description: String,
    pub category: Option<String>,
    pub out_ms: Option<u32>,
    pub opaque: bool,
    /// The most frames a second the design needs, when it says.
    pub fps: Option<u32>,
    /// The share of the canvas size the page is drawn at and stretched from.
    pub resolution: Option<f64>,
    pub fields: Vec<TemplateField>,
}

/// The JSON text of the block, if the page has one.
pub fn block(html: &str) -> Option<&str> {
    let at = html.find(&format!("id=\"{BLOCK_ID}\"")).or_else(|| html.find(&format!("id='{BLOCK_ID}'")))?;
    let open = html[..at].rfind("<script")?;
    let start = open + html[open..].find('>')? + 1;
    let end = start + html[start..].find("</script>")?;
    Some(&html[start..end])
}

/// Read the block, adding to `out` whatever is wrong with it.
pub fn read(html: &str, out: &mut Problems) -> Option<Meta> {
    let Some(text) = block(html) else {
        out.push(error(
            "there is no <script type=\"application/json\" id=\"gmx-template\"> block, so the mixer cannot tell what the graphic is or which fields it has",
            "add one inside <head>, such as <script type=\"application/json\" id=\"gmx-template\">{\"title\": \"My lower third\", \"fields\": {\"name\": {\"label\": \"Name\", \"default\": \"Ada Lovelace\"}}}</script>",
        ));
        return None;
    };
    let json: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(e) => {
            out.push(error(&format!("the gmx-template block is not JSON: {e}"), "write it as one JSON object: double quotes round every name and every word, no comments, no comma after the last entry"));
            return None;
        }
    };
    let Some(obj) = json.as_object() else {
        out.push(error("the gmx-template block is not a JSON object", "write it as {\"title\": ..., \"fields\": {...}}"));
        return None;
    };
    let text_of = |k: &str| obj.get(k).and_then(Value::as_str).map(str::to_string);
    let out_ms = match obj.get("out_ms") {
        None => None,
        Some(v) => match v.as_u64().filter(|ms| *ms <= MAX_OUT_MS) {
            Some(ms) => Some(ms as u32),
            None => {
                out.push(error(&format!("out_ms is {v}"), &format!("give the length of the way out in milliseconds, a whole number from 0 to {MAX_OUT_MS}, such as 600")));
                None
            }
        },
    };
    Some(Meta {
        title: text_of("title"),
        description: text_of("description").unwrap_or_default(),
        category: text_of("category"),
        out_ms,
        opaque: obj.get("opaque").and_then(Value::as_bool).unwrap_or(false),
        fps: fps(obj.get("fps"), out),
        resolution: resolution(obj.get("resolution"), out),
        fields: fields(text, obj.get("fields"), out),
    })
}

fn fps(v: Option<&Value>, out: &mut Problems) -> Option<u32> {
    let v = v?;
    let fps = v.as_u64().filter(|f| (1..=60).contains(f));
    if fps.is_none() {
        out.push(error(&format!("fps is {v}"), "give the most frames a second the design needs, a whole number from 1 to 60, such as 20 for a slow background"));
    }
    fps.map(|f| f as u32)
}

fn resolution(v: Option<&Value>, out: &mut Problems) -> Option<f64> {
    let v = v?;
    let r = v.as_f64().filter(|r| (0.25..=1.0).contains(r));
    if r.is_none() {
        out.push(error(&format!("resolution is {v}"), "give the share of the canvas size to draw at, 0.25 to 1, such as 0.5 for a soft background; leave it out for full size"));
    }
    r
}

/// The declared fields, in the order the block lists them.
fn fields(text: &str, fields: Option<&Value>, out: &mut Problems) -> Vec<TemplateField> {
    let Some(fields) = fields else { return Vec::new() };
    let Some(map) = fields.as_object() else {
        out.push(error("\"fields\" is not an object", "write \"fields\": {\"name\": {\"label\": \"Name\", \"default\": \"Ada\"}}, one entry a field"));
        return Vec::new();
    };
    let mut list: Vec<(usize, TemplateField)> = Vec::new();
    for (name, decl) in map {
        if !is_name(name) {
            out.push(error(&format!("the field {name:?} has a name the mixer cannot use"), "name a field with lower case letters, digits and _, starting with a letter, such as \"guest_name\""));
            continue;
        }
        if let Some(field) = field(name, decl, out) {
            list.push((text.find(&format!("\"{name}\"")).unwrap_or(usize::MAX), field));
        }
    }
    list.sort_by_key(|(at, _)| *at);
    list.into_iter().map(|(_, f)| f).collect()
}

fn field(name: &str, decl: &Value, out: &mut Problems) -> Option<TemplateField> {
    let get = |k: &str| decl.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string()));
    let kind = match get("type").as_deref() {
        None | Some("text") => FieldType::Text,
        Some("color") | Some("colour") => FieldType::Color,
        Some("image") => FieldType::Image,
        Some(other) => {
            out.push(error(&format!("the field {name} has type {other:?}"), "give a field the type \"text\" (the default), \"color\" or \"image\""));
            return None;
        }
    };
    let default = get("default").unwrap_or_default();
    if kind == FieldType::Color && !default.is_empty() && crate::plugin::kinds::text::style::colour(&default).is_err() {
        out.push(error(&format!("the field {name} is a colour and its default {default:?} is not one"), "write a colour as #rrggbb, such as \"#c8102e\""));
    }
    let label = get("label").unwrap_or_else(|| name.replace('_', " "));
    Some(TemplateField { name: name.to_string(), label, kind, default, fit: None })
}
