//! The request and answer bodies of `gallery.*`.

use super::GalleryItem;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `gallery.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryListRequest {
    /// Words to look for in the name, tags, description and kind:
    /// `"lower third"`, `"red news"`, `"background"`.
    #[serde(default, alias = "search", alias = "q")]
    pub query: Option<String>,
    /// Only this kind: template, image, clip, html, ograf, ticker, text, set.
    #[serde(default, alias = "type")]
    pub kind: Option<String>,
    /// At most this many. Default 50.
    #[serde(default)]
    pub limit: Option<usize>,
}

/// `gallery.save`: one call for any kind. Give exactly one of `svg`, `html`,
/// `data`, `file`, `source` or `set`; the kind is worked out from it.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GallerySaveRequest {
    /// What people call it: `"Storm warning lower third"`. The id is made
    /// from it.
    #[serde(default, alias = "title")]
    pub name: String,
    /// template, image, clip, html, ograf, ticker, text or set. Usually left
    /// out: it is worked out from what you give.
    #[serde(default, alias = "type")]
    pub kind: Option<String>,
    /// A whole SVG document. With `{{fields}}` in it, it is a template;
    /// without, a picture.
    #[serde(default, alias = "svg_code", alias = "svg_text")]
    pub svg: Option<String>,
    /// A whole HTML page, with its CSS and script inline. Transparent where
    /// the page has no background.
    #[serde(default, alias = "html_code", alias = "page")]
    pub html: Option<String>,
    /// A picture or a clip as base64, or as a `data:` URI.
    #[serde(default, alias = "base64", alias = "image")]
    pub data: Option<String>,
    /// The name `data` had, for its type: `logo.png`, `sting.webm`.
    #[serde(default, alias = "file_name")]
    pub filename: Option<String>,
    /// A file or a folder on the mixer's machine: an SVG, a picture, a clip,
    /// an HTML page or a folder holding one, an OGraf package, a zip.
    #[serde(default, alias = "path")]
    pub file: Option<String>,
    /// More files an HTML page loads, by name: text, or a `data:` URI.
    #[serde(default)]
    pub files: Option<Map<String, Value>>,
    /// A ticker or text source as `source.add` takes it: `{"uri":
    /// "ticker:", "params": {...}}`.
    #[serde(default)]
    pub source: Option<Value>,
    /// A virtual set: `{"background": ..., "foreground": ..., "settings":
    /// {...}}`. Each picture is a gallery id, a media file, a path or a
    /// `data:` URI.
    #[serde(default)]
    pub set: Option<SetSpec>,
    /// What to fill a template's fields with, by name.
    #[serde(default, alias = "fields")]
    pub values: Option<Map<String, Value>>,
    /// Words to find it by: `["news", "red"]`, or `"news, red"`.
    #[serde(default)]
    pub tags: Option<Value>,
    #[serde(default)]
    pub description: Option<String>,
    /// Where it goes when placed: full (a background), lower-third, bug,
    /// top, bottom, center, overlay. Worked out when left out.
    #[serde(default, alias = "position", alias = "placement")]
    pub zone: Option<String>,
    /// Say it moves, or does not, when the gallery would guess wrong.
    #[serde(default)]
    pub moves: Option<bool>,
    /// Say it has transparency, or not, when the gallery would guess wrong.
    #[serde(default)]
    pub transparent: Option<bool>,
    /// Who made it, in a word: `claude-code`, `opencode`, `pi`.
    #[serde(default, alias = "author", alias = "agent")]
    pub made_by: Option<String>,
    /// Write over an item with the same id. Every source drawing it is drawn
    /// again.
    #[serde(default, alias = "overwrite")]
    pub replace: bool,
}

/// The pictures and settings of a virtual set.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetSpec {
    /// The plate behind the presenter.
    #[serde(alias = "plate", alias = "back")]
    pub background: String,
    /// A desk or a frame in front of the presenter, transparent elsewhere.
    #[serde(default, alias = "front", alias = "desk")]
    pub foreground: Option<String>,
    /// The layout. Default `virtual-set`.
    #[serde(default)]
    pub layout: Option<String>,
    /// The layout's settings: `presenter_scale` (0.3 to 1), `presenter_x`
    /// (0 to 1), `screen` (green, blue, none).
    #[serde(default)]
    pub settings: Map<String, Value>,
}

/// The answer to `gallery.save`, `gallery.edit` and `gallery.duplicate`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GallerySaved {
    pub item: GalleryItem,
    /// The folder it was written to.
    pub path: String,
    /// Sources drawing it that were drawn again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub redrawn: Vec<String>,
    /// Things that did not stop the save and are worth fixing.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    /// What to call next.
    pub next: String,
}

/// One item by id: `gallery.remove`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryIdRequest {
    /// The item's id from `gallery.list`.
    #[serde(default, alias = "name")]
    pub id: String,
}

/// `gallery.edit`: change what is said about an item, or its field values.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GalleryEditRequest {
    #[serde(default)]
    pub id: String,
    #[serde(default, alias = "title")]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Option<Value>,
    #[serde(default, alias = "position")]
    pub zone: Option<String>,
    /// Field values to keep with the item; `null` drops one.
    #[serde(default, alias = "fields")]
    pub values: Option<Map<String, Value>>,
}

/// `gallery.duplicate`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryDuplicateRequest {
    #[serde(default, alias = "from")]
    pub id: String,
    /// The copy's name. Default: the name with "copy" after it.
    #[serde(default)]
    pub name: Option<String>,
}
