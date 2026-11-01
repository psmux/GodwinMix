//! Looks: LUTs and the presets made from them.
//!
//! A LUT is a table that turns one colour into another. The mixer reads
//! `.cube`, `.3dl` and PNG LUTs, ships camera conversions from log to Rec.709
//! and a set of looks made in code, and keeps what is imported in the
//! library's `luts/` folder. A preset is a LUT, an amount and a colour
//! correction saved under one name. `lut/filter` puts either on an item, a
//! source or the programme. `docs/reference/luts.md` has every method.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One LUT the library has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LutEntry {
    /// The slug a filter names it by: `lut = "teal-orange"`.
    pub name: String,
    pub title: String,
    /// `camera` for a log to Rec.709 conversion, `look` for a built in grade,
    /// `imported` for one brought in with `lut.import`.
    pub group: String,
    /// Points a side of its cube, or the length of a 1D table.
    pub size: usize,
    /// For a built in, what it is for.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub about: String,
}

/// A saved look: a LUT, how much of it, and a colour correction before it.
/// Anything left out is the filter's default.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutPreset {
    /// A slug, made from what is given: "Church Warm" is kept as `church-warm`.
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// The LUT, by name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lut: Option<String>,
    /// 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    /// -1 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    /// -4 to 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
    /// -3 to 3.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gamma: Option<f64>,
    /// -1 to 5.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saturation: Option<f64>,
    /// -180 to 180.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hue_shift: Option<f64>,
}

/// `lut.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutListRequest {
    /// `camera`, `look` or `imported` to see only those.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutList {
    pub luts: Vec<LutEntry>,
    pub presets: Vec<LutPreset>,
    /// Files in the library that would not read, each with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// What `lut.import` reads.
    pub formats: Vec<String>,
}

/// `lut.import`: a file on the mixer's machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutImportRequest {
    /// An absolute path, or a name in the media library (where `media.upload`
    /// puts a file): a `.cube`, a `.3dl` or a `.png`.
    pub path: String,
    /// The slug to keep it under. Taken from the file name when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What the UI calls it. The file's own title, or its name, when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LutImported {
    pub lut: LutEntry,
}

/// `lut.remove` and `lut.preset.remove`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutNameRequest {
    pub name: String,
}

/// `lut.preview`: small pictures of looks, on a source or on a test card.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct LutPreviewRequest {
    /// The source to show them on. A test card of skin, sky, foliage and
    /// greys when absent or when the source has no picture yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// LUT names. Every LUT in the library when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub luts: Option<Vec<String>>,
    /// Preset names, previewed after the LUTs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub presets: Vec<String>,
    /// The amount each LUT is shown at, 0 to 1. 1 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<f64>,
    /// Width of each picture in pixels, 64 to 640. 240 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LutPreview {
    /// The LUT or preset name.
    pub name: String,
    /// `lut` or `preset`.
    pub kind: String,
    /// A JPEG as a data URL, ready for an `<img src>`.
    pub image: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LutPreviews {
    /// The source the pictures show, or absent for the test card.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The picture with no look, to compare against.
    pub original: String,
    pub previews: Vec<LutPreview>,
}
