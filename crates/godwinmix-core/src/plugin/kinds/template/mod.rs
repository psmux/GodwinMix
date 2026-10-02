//! `template/source`: a designed graphic, an SVG with named fields, drawn
//! by the mixer itself.
//!
//! A kind of its own rather than more params on `image/source`, for three
//! reasons. An SVG still is a file and nothing else, and stays as cheap and
//! simple as it is. A template is found by name (`template:score-bug`), in
//! the pack or the library, where a picture is found by path. And its params
//! are its fields, which change on air through `source.set` and are what a
//! form, an agent and a data feed set, so they want a schema and a status of
//! their own.
//!
//! Rendered on the overlay worker by `graphics::render`, once per change of
//! a field or of the size it is drawn at, and held. See `graphics`.

use super::rendered::{self, Rendering};
use crate::config::Params;
use crate::graphics::{self, Template, Values};
use crate::overlay::worker::Rendered;
use crate::overlay::Motion;
use crate::plugin::source::Provide;
use crate::plugin::{Capability, CapabilitySet, Manifest, MediaDecl, ProvideKind, StreamMode, Tier, API_LEVEL};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::sync::Arc;

pub const MANIFEST: Manifest = Manifest {
    plugin: "template",
    id: "source",
    kind: ProvideKind::Source,
    api: API_LEVEL,
    description: "A designed graphic: an SVG with named fields, drawn once per change with no browser",
    uri_schemes: &["template:"],
    rank: 200,
    media: MediaDecl { video: StreamMode::Raw, audio: StreamMode::Raw, alpha: true, thumb: true },
    capabilities: CapabilitySet::new().with(Capability::RestartInPlace).with(Capability::Health).with(Capability::Alpha),
    latency_ms: 0,
    tier: Tier::Core,
};

pub const PROVIDE: Provide = Provide { manifest: MANIFEST, claims, make: rendered::make::<TemplateParams> };

const SCHEME: &str = "template:";

fn claims(uri: &str) -> Option<u16> {
    uri.trim_start().to_ascii_lowercase().starts_with(SCHEME).then_some(MANIFEST.rank)
}

/// A template and the values its source set.
#[derive(Debug, Clone, PartialEq)]
pub struct TemplateParams {
    pub template: Arc<Template>,
    pub values: Values,
}

/// The largest a template is drawn before anything has placed it.
const FIRST_DRAW_MAX: (u32, u32) = (1920, 1080);

/// The template name in a `template:` address.
pub fn name_in(uri: &str) -> Option<&str> {
    let uri = uri.trim();
    uri.get(..SCHEME.len()).filter(|s| s.eq_ignore_ascii_case(SCHEME)).map(|_| uri[SCHEME.len()..].trim())
}

/// Read and check `template/source` params: the template the address names,
/// and `fields`, every one of them a field it has.
pub fn validate(params: &Params) -> Result<TemplateParams> {
    let uri = params.get("uri").and_then(|v| v.as_str()).unwrap_or("");
    let name = name_in(uri).filter(|n| !n.is_empty()).with_context(|| {
        format!("a template source's address is template:<name>, such as template:news-lower-third; got {uri:?}. template.list names them all")
    })?;
    if let Some(key) = params.keys().find(|k| *k != "uri" && *k != "fields") {
        bail!("template/source has no param {key:?}. It takes fields, a table of the template's field values");
    }
    let template = graphics::pack::load(name)?;
    let values = values(params.get("fields"))?;
    graphics::fill::check(&template, &values)?;
    Ok(TemplateParams { template: Arc::new(template), values })
}

/// `params.fields` as strings. A number or a true or false is taken as the
/// words it would be written as, so a score can be sent as 2.
fn values(fields: Option<&toml::Value>) -> Result<Values> {
    let Some(fields) = fields else { return Ok(Values::new()) };
    let table = fields.as_table().context("params.fields is a table of field names and their values, such as { headline = \"Polls close\" }")?;
    table
        .iter()
        .map(|(k, v)| {
            let s = match v {
                toml::Value::String(s) => s.clone(),
                toml::Value::Integer(i) => i.to_string(),
                toml::Value::Float(f) => f.to_string(),
                toml::Value::Boolean(b) => b.to_string(),
                other => bail!("params.fields.{k} is a {}; a field's value is words, a number or a colour", other.type_str()),
            };
            Ok((k.clone(), s))
        })
        .collect()
}

/// The params schema `protocol.json` lists for this kind.
pub fn schema() -> Value {
    json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "title": "template/source params",
        "type": "object",
        "properties": {
            "fields": {
                "description": "The template's field values by name, such as {\"headline\": \"Polls close at ten\"}. A field left out shows the station's brand colour (accent, text, panel) or the template's default. Change one on air with source.set and params.fields.<name>; template.list and template.fields name every field.",
                "type": "object",
                "additionalProperties": { "type": ["string", "number", "boolean"] }
            }
        }
    })
}

impl Rendering for TemplateParams {
    fn manifest() -> &'static Manifest {
        &MANIFEST
    }

    fn validate(params: &Params) -> Result<Self> {
        validate(params)
    }

    fn render(&self, drawn: Option<(u32, u32)>) -> Result<Rendered> {
        let natural = (self.template.info.width, self.template.info.height);
        let size = drawn.filter(|d| d.0 > 1 && d.1 > 1).unwrap_or_else(|| contain(natural, FIRST_DRAW_MAX));
        let picture = graphics::render(&self.template, &self.values, size)?;
        Ok(Rendered { picture: Some(picture), motion: Motion::Still, backdrop: None })
    }
}

/// `size` shrunk to fit inside `max`, keeping its shape.
fn contain(size: (u32, u32), max: (u32, u32)) -> (u32, u32) {
    let s = (max.0 as f64 / size.0.max(1) as f64).min(max.1 as f64 / size.1.max(1) as f64).min(1.0);
    (((size.0 as f64 * s).round() as u32).max(2), ((size.1 as f64 * s).round() as u32).max(2))
}

#[cfg(test)]
mod tests;
