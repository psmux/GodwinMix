//! Transitions and effects from packs: stinger clips with alpha, light leaks
//! and film burns on black, luma matte wipes and GLSL shader transitions.
//!
//! Each one lives in the media library as a folder of its own,
//! `fx/<name>/`, holding the media file and an `fx.json` that is a
//! [`FxManifest`]. That folder is the whole format: a person or an agent can
//! write one by hand, copy one to another machine, or zip a few and import
//! the zip. `docs/reference/fx.md` has the format and every method.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a file is, which decides how it is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FxKind {
    /// A clip with an alpha channel (WebM VP8 or VP9 alpha, ProRes 4444,
    /// QuickTime Animation, PNG in a MOV) drawn over both scenes, with the
    /// cut underneath where it covers the picture.
    Stinger,
    /// A clip on black meant for Screen or Add: a light leak, bokeh, a film
    /// burn, flames, dust. A transition when it whites the picture out, an
    /// effect over the programme either way.
    Overlay,
    /// A black to white picture or clip: the new scene shows through where
    /// the matte is darker than the progress.
    Matte,
    /// A GLSL transition in the gl-transitions form: `vec4 transition(vec2
    /// uv)` with `getFromColor`, `getToColor`, `progress` and `ratio`.
    Shader,
}

/// How a clip is put over the picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    /// Over, by the clip's own alpha. What a stinger with alpha wants.
    #[default]
    Normal,
    /// Lightens only: black leaves the picture as it was. Light leaks, bokeh.
    Screen,
    /// Adds the clip's light to the picture. Flames, sparks, film burn.
    Add,
    /// The clip's brightness is its alpha: black is clear, white is solid.
    Luma,
}

/// `fx.json`: one transition or effect, as its folder describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FxManifest {
    /// The slug every method and a take names it by, such as `light-leak`.
    pub name: String,
    /// What a picker shows.
    #[serde(default)]
    pub title: String,
    pub kind: FxKind,
    #[serde(default)]
    pub blend: Blend,
    /// The media file, inside the folder: a clip, a picture or a `.glsl`.
    pub file: String,
    /// How long it runs. A clip's own length; a matte or a shader's default,
    /// which a take may override with `duration_ms`.
    pub duration_ms: u64,
    /// When the scenes swap under a clip, in milliseconds from its start.
    /// Absent is `cut_at_measured_ms`, then half way.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_at_ms: Option<u64>,
    /// The frame the import found most covered, which is where the cut goes
    /// unless `cut_at_ms` says otherwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cut_at_measured_ms: Option<u64>,
    /// How much of the picture the clip covers at that frame, 0 to 1. Under
    /// about 0.9 the cut may show; the import says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coverage: Option<f64>,
    /// A matte's soft edge, 0 (hard) to 1. 0.1 when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub softness: Option<f64>,
    /// A matte read white first instead of black first.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub invert: bool,
    /// Whether a take may use it.
    #[serde(default = "yes")]
    pub transition: bool,
    /// Whether `fx.fire` may play it over the programme on its own.
    #[serde(default)]
    pub effect: bool,
    /// Where it came from and on what terms, as the pack said.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub licence: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

fn yes() -> bool {
    true
}

/// One item in `fx.list`: the manifest, and what this machine makes of it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FxEntry {
    #[serde(flatten)]
    pub manifest: FxManifest,
    /// `starter` (shipped with the mixer, read only) or `library`.
    pub origin: String,
    /// The folder on the mixer's machine.
    pub dir: String,
    /// `cpu`, `gpu`, or `fade` for a shader this machine can only run as a
    /// dissolve. Absent for a clip or a matte, which always run on the CPU.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runs: Option<String>,
    /// A moving preview: a strip of frames in one JPEG, see `fx.preview`.
    pub preview: String,
    /// Anything an operator should know, in a sentence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[path = "fx_requests.rs"]
mod requests;
pub use requests::*;
