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
    /// A picture: the file name of an image in the media library, such as
    /// `logo.png`. HTML templates only.
    Image,
}

/// What a template is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TemplateFormat {
    /// An SVG drawn by the mixer once per change and held: a still, the
    /// cheapest designed graphic there is.
    #[default]
    Svg,
    /// An HTML page drawn by the browser renderer: CSS animation, canvas,
    /// WebGL, with a way in and a way out. Added by `html:<name>`.
    Html,
}

fn is_svg(f: &TemplateFormat) -> bool {
    *f == TemplateFormat::Svg
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
    /// `svg` (absent) or `html`.
    #[serde(default, skip_serializing_if = "is_svg")]
    pub format: TemplateFormat,
    /// What sort of graphic it is, for a picker: lower-third, ticker, bug,
    /// score, title, background, foreground, countdown, slate. HTML only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// How long its own way out takes, in milliseconds. Give the scene item
    /// `"exit": {"type": "hold", "duration_ms": <this>}` so it stays drawn
    /// while it plays. HTML only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub out_ms: Option<u32>,
    /// True for a design that covers the whole picture on purpose: a
    /// background, a title card, a slate. HTML only.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub opaque: bool,
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
    pub name: String,
}

/// A template and its SVG.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateDoc {
    #[serde(flatten)]
    pub info: TemplateInfo,
    /// The SVG as written, with its `{{field}}` markers in place. Empty for
    /// an HTML template.
    pub svg: String,
    /// The HTML as written, for an HTML template.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

/// `template.save`: check an SVG template and write it into the media
/// library.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateSaveRequest {
    /// The file name, ending `.svg` or not (it is added). One segment, no
    /// slashes.
    pub name: String,
    /// The whole SVG document. Leave it empty and give `html` to save an
    /// HTML template.
    #[serde(default)]
    pub svg: String,
    /// The whole HTML document of an HTML template, saved as `<name>.html`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
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

/// `template.check`: read a template the way `template.save` and
/// `source.add` would, without writing anything, and say what to fix.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TemplateCheckRequest {
    /// An SVG template's whole document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub svg: Option<String>,
    /// An HTML template's whole document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
    /// Or a template by name, as `template.list` gives it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// One thing wrong with a template, and what to do about it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateProblem {
    /// `error` stops it being saved or drawn; `warning` is drawn as it is.
    pub level: String,
    /// What is wrong, quoting the part of the file it is in.
    pub problem: String,
    /// What to change, in words a model can act on.
    pub fix: String,
}

/// The answer to `template.check`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct TemplateChecked {
    /// True when nothing at level `error` was found.
    pub ok: bool,
    pub problems: Vec<TemplateProblem>,
    /// The template as it reads, when it reads at all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<TemplateInfo>,
}
