//! Reading a template: its title, its fields and the size it was drawn at.
//!
//! A field is any `{{name}}` in the SVG. A `<gmx:field>` in the SVG's
//! `<metadata>` gives one a label, a type and a default; a field with no
//! declaration is text, labelled by its name, empty until set. A `<text>`
//! with `data-fit-width` marks every field inside it as shrunk to fit.

use super::xml;
use crate::plugin::kinds::image_probe::svg_size;
use anyhow::{bail, Result};
pub use godwinmix_protocol::graphics::{FieldType, TemplateField, TemplateInfo, TemplateOrigin};

/// The largest template read, in bytes. A logo embedded as a data URI is
/// the usual reason one is big.
pub const MAX_BYTES: usize = 4 << 20;
/// The most fields one template may have.
pub const MAX_FIELDS: usize = 64;
/// The three colours a station rebrands the pack by.
pub const BRAND: [&str; 3] = ["accent", "text", "panel"];

/// A template read and checked, ready to fill.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub info: TemplateInfo,
    pub svg: String,
}

/// One `{{name}}` in the SVG: where it is and what it names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Marker {
    pub start: usize,
    pub end: usize,
    pub name: String,
}

impl Template {
    /// Read `svg` as the template `name`, refusing what could not render.
    pub fn parse(name: &str, origin: TemplateOrigin, svg: String) -> Result<Template> {
        if svg.len() > MAX_BYTES {
            bail!("template {name} is {} bytes; a template may be at most {MAX_BYTES}. Link a logo from the media library rather than embedding a large one", svg.len());
        }
        let Some((width, height)) = svg_size(&svg) else {
            bail!("template {name} is not an SVG with a size. Give its <svg> element a viewBox, such as viewBox=\"0 0 1920 1080\"");
        };
        super::images::check_refs(name, &svg)?;
        let fields = fields(name, &svg)?;
        let meta = xml::tags(&svg, "gmx:template").first().map(|t| t.text);
        let title = meta.and_then(|t| xml::attr(t, "title")).unwrap_or_else(|| name.trim_end_matches(".svg").replace(['-', '_'], " "));
        let description = meta.and_then(|t| xml::attr(t, "description")).unwrap_or_default();
        let uri = format!("template:{name}");
        let info = TemplateInfo { name: name.into(), title, description, origin, uri, width, height, fields };
        Ok(Template { info, svg })
    }

    pub fn field(&self, name: &str) -> Option<&TemplateField> {
        self.info.fields.iter().find(|f| f.name == name)
    }

    pub fn field_names(&self) -> Vec<String> {
        self.info.fields.iter().map(|f| f.name.clone()).collect()
    }
}

/// Every `{{name}}` in `svg`, in order. A `{{` that does not close on a
/// plain name is left alone as text.
pub fn markers(svg: &str) -> Vec<Marker> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = svg[from..].find("{{").map(|i| i + from) {
        from = at + 2;
        let Some(len) = svg[from..].find("}}") else { break };
        let name = &svg[from..from + len];
        if is_name(name) {
            out.push(Marker { start: at, end: from + len + 2, name: name.to_string() });
            from += len + 2;
        }
    }
    out
}

/// A field name: a lower case letter, then letters, digits and `_`, at
/// most 40 in all.
pub fn is_name(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('a'..='z'))
        && s.len() <= 40
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

/// The declared fields, then any marker not declared, each once.
fn fields(name: &str, svg: &str) -> Result<Vec<TemplateField>> {
    let mut out: Vec<TemplateField> = Vec::new();
    for tag in xml::tags(svg, "gmx:field") {
        let Some(field) = xml::attr(tag.text, "name") else { bail!("template {name} has a <gmx:field> with no name") };
        if !is_name(&field) {
            bail!("template {name} declares a field {field:?}; a field name is lower case letters, digits and _, starting with a letter");
        }
        let kind = match xml::attr(tag.text, "type").as_deref() {
            None | Some("text") => FieldType::Text,
            Some("color") | Some("colour") => FieldType::Color,
            Some(other) => bail!("template {name} field {field} has type {other:?}; a field is \"text\" or \"color\""),
        };
        let label = xml::attr(tag.text, "label").unwrap_or_else(|| label_of(&field));
        let default = xml::attr(tag.text, "default").unwrap_or_default();
        out.retain(|f| f.name != field);
        out.push(TemplateField { name: field, label, kind, default, fit: None });
    }
    for m in markers(svg) {
        if !out.iter().any(|f| f.name == m.name) {
            let kind = if BRAND.contains(&m.name.as_str()) { FieldType::Color } else { FieldType::Text };
            out.push(TemplateField { label: label_of(&m.name), name: m.name, kind, default: String::new(), fit: None });
        }
    }
    for (field, width) in super::fit::fitted_fields(svg) {
        if let Some(f) = out.iter_mut().find(|f| f.name == field) {
            f.fit = Some(width);
        }
    }
    if out.len() > MAX_FIELDS {
        bail!("template {name} has {} fields; a template may have at most {MAX_FIELDS}", out.len());
    }
    Ok(out)
}

/// `score_home` as `Score home`.
fn label_of(name: &str) -> String {
    let words = name.replace('_', " ");
    let mut c = words.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}
