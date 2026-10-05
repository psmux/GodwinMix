//! `html/graphic` params: the template, its field values, and whether it
//! follows the programme in and out or is held one way.

use crate::config::Params;
use crate::graphics::html::{self, pack, HtmlTemplate};
use crate::graphics::template::FieldType;
use crate::graphics::{brand, fill, Values};
use anyhow::{bail, Context, Result};
use serde_json::{json, Map, Value};
use std::sync::Arc;

/// When the graphic plays its way in and its way out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Cue {
    /// In when an item showing it goes on the programme, out when the last
    /// one comes off. The default.
    #[default]
    Auto,
    /// Held in, whatever the programme does: for looking at it on a scene
    /// that is not on air, or a graphic driven by hand.
    In,
    /// Held out.
    Out,
}

/// The highest rate the renderer is asked to paint at.
pub const MAX_FPS: u32 = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct HtmlParams {
    pub template: Arc<HtmlTemplate>,
    pub values: Values,
    pub cue: Cue,
    /// The most frames a second the page is painted at. Zero for the canvas
    /// rate.
    pub fps: u32,
}

pub fn validate(params: &Params) -> Result<HtmlParams> {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    let name = html::name_in(uri).filter(|n| !n.is_empty()).with_context(|| {
        format!("an HTML template's address is html:<name>, such as html:lower-third-glass; got {uri:?}. template.list names them all")
    })?;
    if let Some(key) = params.keys().find(|k| !matches!(k.as_str(), "uri" | "fields" | "cue" | "fps")) {
        bail!("html/graphic has no param {key:?}. It takes fields (the template's field values), cue (auto, in or out) and fps");
    }
    let template = pack::load(name)?;
    let values = super::super::template::values_of(params.get("fields"))?;
    fill::check(&template.as_template(), &values)?;
    let cue = match params.get("cue").and_then(|v| v.as_str()).unwrap_or("auto") {
        "auto" => Cue::Auto,
        "in" => Cue::In,
        "out" => Cue::Out,
        other => bail!("params.cue is {other:?}; it is auto (in and out with the programme, the default), in or out"),
    };
    let fps = match params.get("fps") {
        None => 0,
        Some(v) => v.as_integer().filter(|f| (1..=MAX_FPS as i64).contains(f)).map(|f| f as u32).with_context(|| {
            format!("params.fps is a whole number of frames a second from 1 to {MAX_FPS}, or leave it out for the canvas rate")
        })?,
    };
    Ok(HtmlParams { template: Arc::new(template), values, cue, fps })
}

impl HtmlParams {
    /// Whether the graphic is in, given whether the programme shows it.
    pub fn is_in(&self, on_air: bool) -> bool {
        match self.cue {
            Cue::Auto => on_air,
            Cue::In => true,
            Cue::Out => false,
        }
    }

    /// The whole state the page is told, as one JSON line: every field's
    /// value (the source's, the brand's, the default) and in or out.
    pub fn state(&self, on_air: bool) -> String {
        let brand = brand::brand();
        let library = brand::library();
        let mut fields = Map::new();
        for f in &self.template.info.fields {
            let (value, _) = fill::value_of(f, &self.values, &brand);
            let value = if f.kind == FieldType::Image { image_url(&value, library.as_deref()) } else { value };
            fields.insert(f.name.clone(), Value::String(value));
        }
        let cue = if self.is_in(on_air) { "in" } else { "out" };
        json!({ "fields": fields, "cue": cue }).to_string()
    }
}

/// An image field's value as an address the page can load: a library file
/// name becomes a `file:` address, anything with a scheme is left alone.
fn image_url(value: &str, library: Option<&std::path::Path>) -> String {
    if value.is_empty() || value.contains(':') {
        return value.to_string();
    }
    match library {
        Some(dir) if !value.contains("..") => crate::input::file_uri(&dir.join(value)),
        _ => value.to_string(),
    }
}

/// The params schema `protocol.json` lists for this kind.
pub fn schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "html/graphic params",
        "type": "object",
        "properties": {
            "fields": {
                "description": "The template's field values by name, such as {\"name\": \"Ada Lovelace\"}. Change one on air with source.set and params.fields.<name>; the page is told at once, with no reload. template.list names every field.",
                "type": "object",
                "additionalProperties": { "type": ["string", "number", "boolean"] }
            },
            "cue": {
                "description": "auto (the default): the graphic plays its way in when an item showing it goes on the programme and its way out when it comes off. in or out holds it there, to look at it off air or drive it by hand.",
                "enum": ["auto", "in", "out"]
            },
            "fps": {
                "description": "The most frames a second the page is painted at, 1 to 60. Leave it out for the canvas rate; a lower rate costs less for a graphic that moves slowly.",
                "type": "integer", "minimum": 1, "maximum": MAX_FPS
            }
        }
    })
}
