//! Graphic templates: SVGs with named fields, drawn by `template/source`.
//! A field's value lives in the source's params at `params.fields.<name>`,
//! which is what any client, a data feed included, sets with `source.set`.

use serde::{Deserialize, Serialize};

/// What a field holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    /// Words. Escaped for XML, so `&` and `<` show as themselves.
    #[default]
    Text,
    /// A colour: `#rgb`, `#rrggbb` or `#rrggbbaa`.
    Color,
}

/// One named field of a template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateField {
    /// The name in `{{name}}` and in `params.fields`: lower case letters,
    /// digits and underscores.
    pub name: String,
    /// What a form calls it.
    pub label: String,
    #[serde(rename = "type")]
    pub kind: FieldType,
    /// What it shows when the source's params do not say. For `accent`,
    /// `text` and `panel` the station's brand colours come before this.
    pub default: String,
    /// The width, in the template's own units, a text holding this field is
    /// shrunk to fit inside. Absent for a field that is never shrunk.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<f64>,
}

/// Where a template comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TemplateOrigin {
    /// The starter pack built into the mixer. Read only: copy one with
    /// `template.get` and `template.save` to change it.
    Pack,
    /// An SVG in the media library.
    Library,
}

/// One template, as `template.list` and `template.get` describe it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateInfo {
    /// The name `template:<name>` adds it by: a pack name such as
    /// `news-lower-third`, or a library file name such as `my-bar.svg`.
    pub name: String,
    pub title: String,
    pub description: String,
    pub origin: TemplateOrigin,
    /// The address to give `source.add`.
    pub uri: String,
    /// The size the SVG declares, which is the canvas it was designed on.
    pub width: u32,
    pub height: u32,
    pub fields: Vec<TemplateField>,
}

/// The answer to `template.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateList {
    pub templates: Vec<TemplateInfo>,
    /// Library files that look like templates and would not read, and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}

/// `template.get`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateGetRequest {
    /// A pack name or a library file name, as `template.list` gives it.
    /// The REST layer puts it in the path as `id`, so both are read.
    #[serde(alias = "id")]
    pub name: String,
}

/// A template and its SVG.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateDoc {
    #[serde(flatten)]
    pub info: TemplateInfo,
    /// The SVG as written, with its `{{field}}` markers in place.
    pub svg: String,
}

/// `template.save`: check an SVG template and write it into the media
/// library.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateSaveRequest {
    /// The file name, ending `.svg` or not (it is added). One segment, no
    /// slashes. `id` in a REST path.
    #[serde(alias = "id")]
    pub name: String,
    /// The whole SVG document.
    pub svg: String,
    /// Write over a library file of the same name. Every source drawing it
    /// is drawn again with the new SVG, on air, with no rebuild.
    #[serde(default)]
    pub replace: bool,
}

/// The answer to `template.save`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateSaved {
    pub template: TemplateInfo,
    /// Where it was written on the mixer.
    pub path: String,
    /// The sources drawing this template that were drawn again with it.
    pub redrawn: Vec<String>,
}

/// `template.fields`: the fields of a running graphic.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateFieldsRequest {
    /// The source id of a `template/source`.
    pub id: String,
}

/// One field and what it shows now.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FieldValue {
    #[serde(flatten)]
    pub field: TemplateField,
    /// What is on screen: the source's own value, the brand colour or the
    /// default, in that order.
    pub value: String,
    /// Whether the source's params set it, rather than a default.
    pub set: bool,
}

/// The answer to `template.fields`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateFields {
    pub id: String,
    /// The template's name.
    pub template: String,
    pub fields: Vec<FieldValue>,
    /// Where a client sets a field with `source.set`: `params.fields.<name>`.
    pub path: String,
}
