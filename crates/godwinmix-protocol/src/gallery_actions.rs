//! Looking at an item, putting it on air, and moving items between machines.

use super::GalleryItem;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// `gallery.preview`: a picture of an item, drawn on demand.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryPreviewRequest {
    #[serde(alias = "name")]
    pub id: String,
    /// Pixels wide, 64 to 1920. Default 960, which is what reading a lower
    /// third needs.
    #[serde(default)]
    pub width: Option<u32>,
    /// What shows through the transparent parts: `checker` (the default),
    /// `black`, `white`, or a colour `#rrggbb`.
    #[serde(default)]
    pub background: Option<String>,
    /// Field values to try, over the item's own, without saving them.
    #[serde(default, alias = "fields")]
    pub values: Option<Map<String, Value>>,
}

/// The answer to `gallery.preview`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryPreview {
    pub id: String,
    pub width: u32,
    pub height: u32,
    /// The JPEG, base64.
    pub image: String,
    pub encoding: String,
    pub format: String,
    /// How it was made: `drawn` by the mixer now, `poster` (the item's own
    /// preview file), `source` (a frame of a source drawing it), or `card`
    /// (a placeholder naming the kind, when nothing could draw it here).
    pub from: String,
    /// One line about the picture, for a model reading it.
    pub caption: String,
}

/// `gallery.place`: add an item to a scene in its zone. Hidden unless
/// `visible`; `gallery.show` takes it on air.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GalleryPlaceRequest {
    #[serde(alias = "name")]
    pub id: String,
    /// The scene to add it to. Default: the scene on air.
    #[serde(default)]
    pub scene: Option<String>,
    /// Where on the canvas, over the item's own zone: full, lower-third,
    /// bug, top, bottom, center, overlay.
    #[serde(default, alias = "position", alias = "placement")]
    pub zone: Option<String>,
    /// Field values for this placement, over the item's own.
    #[serde(default, alias = "fields")]
    pub values: Option<Map<String, Value>>,
    /// Show it at once. Default false: placed hidden, ready to take.
    #[serde(default, alias = "on_air", alias = "live", alias = "show")]
    pub visible: Option<bool>,
    /// For a set: the camera source standing in it. Default: the source on
    /// air.
    #[serde(default, alias = "presenter")]
    pub camera: Option<String>,
}

/// The answer to `gallery.place`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryPlaced {
    pub id: String,
    pub scene: String,
    /// The source drawing it. For a set, the scene's sources are in `scene`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The scene item's name, for `gallery.show` and `scene.item.set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    pub visible: bool,
    /// True when it was on the scene already, and only its values changed.
    pub updated: bool,
    /// True for a set, which becomes a scene of its own.
    pub new_scene: bool,
    pub next: String,
}

/// `gallery.show`: take a placed item on air, or off.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GalleryShowRequest {
    /// The gallery id, the source id or the scene item's name.
    #[serde(alias = "name", alias = "item")]
    pub id: String,
    /// Default: the scene on air.
    #[serde(default)]
    pub scene: Option<String>,
    /// True to show, false to hide. Default true.
    #[serde(default, alias = "on_air", alias = "show")]
    pub visible: Option<bool>,
}

/// The answer to `gallery.show`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryShown {
    pub scene: String,
    pub item: String,
    pub visible: bool,
    /// For a set: the scene was taken to the programme rather than an item
    /// shown.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub took: bool,
}

/// `gallery.export`: items as one zip to carry to another mixer.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryExportRequest {
    /// The ids, as a list or one string with commas. Default: every item
    /// that was not shipped with the mixer.
    #[serde(default)]
    pub ids: Option<Value>,
    /// Where to write the zip on the mixer. Default: the gallery's
    /// `exports` folder.
    #[serde(default)]
    pub path: Option<String>,
}

/// The answer to `gallery.export`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryExported {
    pub path: String,
    pub size_bytes: u64,
    pub ids: Vec<String>,
    /// Where a browser downloads it from this mixer.
    pub url: String,
}

/// `gallery.import`: files made elsewhere, checked one by one.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryImportRequest {
    /// A file or a folder on the mixer: a gallery zip, an SVG, an HTML page
    /// or a folder or zip with one, an OGraf package, a PNG or WebP, a WebM
    /// or MOV. A folder of several of these imports each.
    #[serde(default, alias = "file")]
    pub path: Option<String>,
    /// The same as base64, with `filename`.
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    /// Write over items with the same id.
    #[serde(default)]
    pub replace: bool,
}

/// One file the import would not take.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Refused {
    pub file: String,
    /// What was wrong with it.
    pub reason: String,
    /// What to do about it.
    pub fix: String,
}

/// The answer to `gallery.import`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryImported {
    pub added: Vec<GalleryItem>,
    pub refused: Vec<Refused>,
}
