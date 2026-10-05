//! The bodies of the `fx.*` methods.

use super::{FxBlend, FxEntry, FxKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `fx.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxListRequest {
    /// `transition` or `effect` to see only those. Absent lists all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// What `fx.list` answers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxList {
    pub fx: Vec<FxEntry>,
    /// Folders under `fx/` that would not read, each with the reason.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
    /// Whether GStreamer GL runs here, which decides `runs` for a shader.
    pub gpu: bool,
}

/// `fx.import`: a file, a folder or a zip on the mixer's machine.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxImportRequest {
    /// An absolute path, or a name in the media library (where `media.upload`
    /// puts a file). A folder or a zip imports everything in it it can read.
    pub path: String,
    /// The slug to give it. Taken from the file name when absent. Ignored for
    /// a folder or a zip, whose items are named after their files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// What it is, when the import should not decide by looking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<FxKind>,
    /// How a clip is put over the picture, when the import should not decide.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<FxBlend>,
    /// Where the scenes swap, when the measured frame is not the one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_at_ms: Option<u64>,
    /// Write over an item of the same name.
    #[serde(default)]
    pub replace: bool,
}

/// What `fx.import` answers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxImported {
    pub imported: Vec<FxEntry>,
    /// Files that were not imported, each with why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skipped: Vec<FxSkipped>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FxSkipped {
    pub file: String,
    pub reason: String,
}

/// `fx.set`: change what an item does. Only what is named moves.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxSetRequest {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<FxBlend>,
    /// Where the scenes swap. 0 puts it back to the measured frame.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub softness: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invert: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effect: Option<bool>,
}

/// `fx.get`, `fx.remove` and `fx.preview`: one item by name.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxNameRequest {
    pub name: String,
}

/// `fx.fire`: play an effect over the programme once.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxFireRequest {
    pub name: String,
    /// A blend for this firing only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blend: Option<FxBlend>,
    /// How strong, 0 to 1. 1 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
}

/// What `fx.fire` answers.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxFired {
    pub name: String,
    /// How long it will be on the programme.
    pub duration_ms: u64,
}

/// What `fx.preview` answers: a strip of frames side by side in one JPEG.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct FxPreview {
    pub name: String,
    /// `GET` this for the JPEG.
    pub url: String,
    pub frames: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    /// How long the strip takes to play once, in milliseconds.
    pub duration_ms: u64,
}
