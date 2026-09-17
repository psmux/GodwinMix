//! Reading an OBS Studio scene collection.
//!
//! An operator with a working OBS layout will not retype it, so this reads the
//! collection file OBS already wrote and produces a scene document plus a
//! source list. It is one command and it never asks a question: everything it
//! cannot carry across is reported rather than guessed at, because a silent
//! partial import is worse than no import (11 section 7).
//!
//! What OBS stores, read from `obs-scene.c` and a real exported collection:
//! `sources` is a flat array of every source, including the scenes themselves
//! (`id` of `scene`) and the groups (`id` of `group`). A scene's `settings`
//! carries `items`, and an item carries `pos`, `rot`, `scale`, `align`,
//! `bounds_type`, `bounds`, `bounds_align`, the four pixel crops, `visible`,
//! `locked` and `blend_type`. Items name their source by `source_uuid` with a
//! fallback to `name`, which is the half finished identity migration of PR
//! 8345 and the reason both are tried here.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::document::*;

mod build;
use build::Importer;

#[cfg(test)]
mod import_tests;

/// What became of one OBS source.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum Outcome {
    /// It came across whole.
    Imported { r#type: String, id: String },
    /// It came across, and the plugin that plays it is not installed yet.
    NeedsPlugin { r#type: String, id: String, plugin: String },
    /// It did not come across. `placeholder` names what was put in the scene
    /// in its place, when anything was.
    Skipped {
        reason: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        placeholder: Option<String>,
    },
}

/// One line of the report: an OBS source and what happened to it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SourceReport {
    /// The OBS plugin type, for example `ffmpeg_source`.
    pub obs_type: String,
    pub obs_name: String,
    #[serde(flatten)]
    pub outcome: Outcome,
    /// How many items in the collection use it.
    pub placements: usize,
}

/// A source filter that had to be copied onto each placement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct FilterReport {
    pub filter: String,
    pub obs_type: String,
    pub source: String,
    /// The items it was copied onto, by their path in the document.
    pub placements: Vec<String>,
}

/// Everything the import did, for stdout and for `--report json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Report {
    pub collection: String,
    pub canvas: Canvas,
    pub scenes: usize,
    pub items: usize,
    pub sources: Vec<SourceReport>,
    pub filters_duplicated: Vec<FilterReport>,
    /// Anything the reader should know that is not about one source.
    pub notes: Vec<String>,
}

/// A source in the config the import writes, in the shape of 03 section 3:
/// a plugin qualified `type` and a `params` table, with `uri` kept where one
/// makes sense so today's config still reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ImportedSource {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(skip_serializing_if = "Value::is_null")]
    pub params: Value,
}

/// What one import produced.
#[derive(Debug, Clone)]
pub struct Import {
    pub document: Collection,
    pub sources: Vec<ImportedSource>,
    pub report: Report,
}

impl Import {
    /// The source list as a TOML fragment ready to paste into, or be written
    /// as, a config file.
    pub fn to_config_toml(&self) -> Result<String> {
        #[derive(Serialize)]
        struct Wrapper<'a> {
            sources: &'a [ImportedSource],
        }
        let body = toml::to_string_pretty(&Wrapper { sources: &self.sources })
            .context("writing the imported sources as TOML")?;
        Ok(format!(
            "# Sources imported from the OBS scene collection {:?}.\n\
             # Each one names the plugin that plays it in `type`; install any that are\n\
             # missing with `gmx plugin add <name>`. The scenes are in the scene document\n\
             # beside this file.\n\n{body}",
            self.report.collection
        ))
    }
}

/// Options an operator can pass on the command line.
#[derive(Debug, Clone, Default)]
pub struct Options {
    /// The canvas to import onto. OBS keeps its base canvas in the profile,
    /// not in the collection, so it cannot be read from the file.
    pub canvas: Option<Canvas>,
    /// `name=WIDTHxHEIGHT` hints, so crops and unbounded items land exactly.
    pub source_sizes: BTreeMap<String, (f64, f64)>,
}

/// Read a collection file.
pub fn import(text: &str, options: &Options) -> Result<Import> {
    let raw: ObsCollection = serde_json::from_str(text).context(
        "this file is not an OBS scene collection. Export one from OBS with Scene Collection then Export, and pass that file.",
    )?;
    Importer::new(raw, options).run()
}

// ---------------------------------------------------------------------------
// The OBS side of the wire.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
struct ObsCollection {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    scene_order: Vec<ObsNamed>,
    #[serde(default)]
    sources: Vec<ObsSource>,
    /// Older collections keep groups in their own array.
    #[serde(default)]
    groups: Vec<ObsSource>,
}

#[derive(Debug, Clone, Deserialize)]
struct ObsNamed {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ObsSource {
    #[serde(default)]
    id: String,
    #[serde(default)]
    versioned_id: Option<String>,
    #[serde(default)]
    name: String,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    settings: Value,
    #[serde(default)]
    filters: Vec<ObsFilter>,
}

#[derive(Debug, Clone, Deserialize)]
struct ObsFilter {
    #[serde(default)]
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    settings: Value,
    #[serde(default = "yes")]
    enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
struct ObsItem {
    #[serde(default)]
    name: String,
    #[serde(default)]
    source_uuid: Option<String>,
    #[serde(default = "yes")]
    visible: bool,
    #[serde(default)]
    locked: bool,
    #[serde(default)]
    rot: f64,
    #[serde(default)]
    align: u32,
    #[serde(default)]
    pos: ObsVec2,
    #[serde(default = "unit")]
    scale: ObsVec2,
    #[serde(default)]
    bounds_type: Value,
    #[serde(default)]
    bounds_align: u32,
    #[serde(default)]
    bounds: ObsVec2,
    #[serde(default)]
    crop_left: f64,
    #[serde(default)]
    crop_top: f64,
    #[serde(default)]
    crop_right: f64,
    #[serde(default)]
    crop_bottom: f64,
    #[serde(default)]
    blend_type: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, Deserialize)]
struct ObsVec2 {
    #[serde(default)]
    x: f64,
    #[serde(default)]
    y: f64,
}

fn unit() -> ObsVec2 {
    ObsVec2 { x: 1.0, y: 1.0 }
}

/// OBS's alignment bitmask, from `libobs/obs.h`.
const ALIGN_LEFT: u32 = 1;
const ALIGN_RIGHT: u32 = 2;
const ALIGN_TOP: u32 = 4;
const ALIGN_BOTTOM: u32 = 8;

/// The anchor an OBS alignment bitmask means, as 0, 0.5, 1 factors.
fn anchor_of(align: u32) -> Vec2 {
    let x = if align & ALIGN_LEFT != 0 {
        0.0
    } else if align & ALIGN_RIGHT != 0 {
        1.0
    } else {
        0.5
    };
    let y = if align & ALIGN_TOP != 0 {
        0.0
    } else if align & ALIGN_BOTTOM != 0 {
        1.0
    } else {
        0.5
    };
    Vec2::new(x, y)
}

/// The seven `OBS_BOUNDS_*` values, as an index and as the name OBS writes in
/// the newer files, mapped onto `frame` plus `fit` (11 section 2).
fn fit_of(bounds_type: &Value) -> Option<Fit> {
    let index = match bounds_type {
        Value::Number(n) => n.as_u64()? as u32,
        Value::String(s) => match s.as_str() {
            "OBS_BOUNDS_NONE" => 0,
            "OBS_BOUNDS_STRETCH" => 1,
            "OBS_BOUNDS_SCALE_INNER" => 2,
            "OBS_BOUNDS_SCALE_OUTER" => 3,
            "OBS_BOUNDS_SCALE_TO_WIDTH" => 4,
            "OBS_BOUNDS_SCALE_TO_HEIGHT" => 5,
            "OBS_BOUNDS_MAX_ONLY" => 6,
            _ => return None,
        },
        _ => 0,
    };
    match index {
        0 => None, // no bounds: the item is its own size, scaled
        1 => Some(Fit::Stretch),
        2 => Some(Fit::Contain),
        3 => Some(Fit::Cover),
        4 => Some(Fit::FitWidth),
        5 => Some(Fit::FitHeight),
        6 => Some(Fit::Max),
        _ => None,
    }
}

/// OBS's blend names.
fn blend_of(name: Option<&str>) -> Blend {
    match name.unwrap_or("normal") {
        "additive" | "add" => Blend::Add,
        "screen" => Blend::Screen,
        "multiply" => Blend::Multiply,
        "lighten" => Blend::Lighten,
        "darken" => Blend::Darken,
        "subtract" => Blend::Subtract,
        _ => Blend::Normal,
    }
}

// ---------------------------------------------------------------------------
// The mapping table.
// ---------------------------------------------------------------------------

/// What a GodwinMix type needs before it will run.
#[derive(Debug, Clone, PartialEq)]
enum Mapped {
    /// A type the core has today.
    Core { kind: String, uri: Option<String>, params: Value },
    /// A type that arrives with a plugin nobody has installed yet.
    Plugin { kind: String, plugin: String, params: Value },
    /// Not a source at all: a graphic item in the scene.
    Graphic { graphic: String, params: Value, note: String },
    /// Nothing sensible to do with it.
    Skip { reason: String },
}

/// Map one OBS source onto a GodwinMix type. The table is the whole point of
/// the importer, so it is one function and it reads top to bottom.
fn map_source(source: &ObsSource) -> Mapped {
    let s = &source.settings;
    let text = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    match source.id.as_str() {
        "ffmpeg_source" | "vlc_source" => {
            let path = first_media_path(s);
            if path.is_empty() {
                return Mapped::Skip {
                    reason: "it names no file or address in its settings".into(),
                };
            }
            let kind = kind_for_uri(&path);
            let mut params = json!({ "uri": path });
            if s.get("looping").and_then(Value::as_bool) == Some(true) {
                params["loop"] = Value::Bool(true);
            }
            Mapped::Core { kind, uri: Some(path), params }
        }
        "image_source" => {
            let path = text("file");
            if path.is_empty() {
                return Mapped::Skip { reason: "it names no image file".into() };
            }
            Mapped::Core { kind: "file/source".into(), uri: Some(path.clone()), params: json!({ "uri": path }) }
        }
        "browser_source" => {
            let url = if text("is_local_file").is_empty() && !text("url").is_empty() {
                text("url")
            } else if !text("local_file").is_empty() {
                format!("file://{}", text("local_file"))
            } else {
                text("url")
            };
            if url.is_empty() {
                return Mapped::Skip { reason: "it names no page".into() };
            }
            let mut params = json!({ "url": url });
            for key in ["width", "height", "fps"] {
                if let Some(v) = s.get(key).and_then(Value::as_f64) {
                    params[key] = json!(v);
                }
            }
            if !text("css").is_empty() {
                params["css"] = Value::String(text("css"));
            }
            Mapped::Core { kind: "browser/source".into(), uri: Some(format!("web+{url}")), params }
        }
        "v4l2_input" | "av_capture_input" | "av_capture_input_v2" | "dshow_input" => {
            let device = ["device", "device_id", "device_name", "video_device_id"]
                .iter()
                .map(|k| text(k))
                .find(|v| !v.is_empty())
                .unwrap_or_default();
            Mapped::Plugin {
                kind: "camera/source".into(),
                plugin: "camera".into(),
                params: json!({ "device": device }),
            }
        }
        "monitor_capture" | "display_capture" | "window_capture" | "xshm_input"
        | "xcomposite_input" | "pipewire-screen-capture-source" | "screen_capture" => {
            let mut params = json!({});
            for key in ["monitor", "monitor_id", "window", "capture_window", "display"] {
                if !text(key).is_empty() {
                    params[key] = Value::String(text(key));
                }
            }
            Mapped::Plugin { kind: "screen/source".into(), plugin: "screen".into(), params }
        }
        "text_gdiplus" | "text_gdiplus_v2" | "text_ft2_source" | "text_ft2_source_v2" => {
            Mapped::Graphic {
                graphic: "text/graphic".into(),
                params: json!({
                    "text": text("text"),
                    "font": s.get("font").cloned().unwrap_or(Value::Null),
                    "color": s.get("color").cloned().unwrap_or(Value::Null),
                }),
                note: "a placeholder graphic item, to point at a graphic plugin".into(),
            }
        }
        "color_source" | "color_source_v2" | "color_source_v3" => {
            let colour = s.get("color").and_then(Value::as_u64).unwrap_or(0xff000000);
            Mapped::Core {
                kind: "test/source".into(),
                uri: None,
                params: json!({ "pattern": "solid", "color": obs_colour(colour) }),
            }
        }
        "" => Mapped::Skip { reason: "it has no type".into() },
        other => Mapped::Skip {
            reason: format!("GodwinMix has no equivalent of the OBS source type {other:?} yet"),
        },
    }
}

/// The first setting that looks like a file or an address.
fn first_media_path(settings: &Value) -> String {
    for key in ["local_file", "input", "playlist_file", "url"] {
        if let Some(v) = settings.get(key).and_then(Value::as_str) {
            if !v.is_empty() {
                return v.to_string();
            }
        }
    }
    // A VLC source keeps its files in a playlist of objects.
    settings
        .get("playlist")
        .and_then(Value::as_array)
        .and_then(|list| list.first())
        .and_then(|entry| entry.get("value").or_else(|| entry.get("hidden")))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

/// Which plugin plays a URL, by its scheme. A bare path is a file.
fn kind_for_uri(uri: &str) -> String {
    let scheme = uri.split_once("://").map(|(s, _)| s.to_ascii_lowercase());
    match scheme.as_deref() {
        Some("rtmp") | Some("rtmps") => "rtmp/source",
        Some("srt") => "srt/source",
        Some("rtsp") => "rtsp/source",
        Some("udp") | Some("rtp") => "rtp/source",
        Some("http") | Some("https") if uri.contains(".m3u8") => "hls/source",
        Some("http") | Some("https") => "file/source",
        _ => "file/source",
    }
    .to_string()
}

/// OBS stores a colour as 0xAABBGGRR. A person reads `#rrggbb`.
fn obs_colour(value: u64) -> String {
    let (r, g, b) = (value & 0xff, (value >> 8) & 0xff, (value >> 16) & 0xff);
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// A source name as an id an operator can type: lower case, words joined by
/// hyphens. Ids stay slugs, even when the nodes around them carry UUIDs.
fn slug(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let out = out.trim_matches('-').to_string();
    if out.is_empty() {
        "source".into()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_names_become_ids_an_operator_can_type() {
        assert_eq!(slug("CAM 1 (Studio)"), "cam-1-studio");
        assert_eq!(slug("lower third"), "lower-third");
        assert_eq!(slug("---"), "source");
        assert_eq!(slug("Bücher"), "b-cher");
    }

    #[test]
    fn the_seven_obs_bounds_types_each_land_on_a_fit() {
        let want = [
            (0u64, None),
            (1, Some(Fit::Stretch)),
            (2, Some(Fit::Contain)),
            (3, Some(Fit::Cover)),
            (4, Some(Fit::FitWidth)),
            (5, Some(Fit::FitHeight)),
            (6, Some(Fit::Max)),
        ];
        for (index, fit) in want {
            assert_eq!(fit_of(&json!(index)), fit, "bounds_type {index}");
        }
        // The newer files write the name instead of the number.
        assert_eq!(fit_of(&json!("OBS_BOUNDS_SCALE_OUTER")), Some(Fit::Cover));
        assert_eq!(fit_of(&json!("OBS_BOUNDS_NONE")), None);
    }

    #[test]
    fn the_alignment_bitmask_becomes_an_anchor() {
        assert_eq!(anchor_of(ALIGN_TOP | ALIGN_LEFT), Vec2::new(0.0, 0.0));
        assert_eq!(anchor_of(0), Vec2::new(0.5, 0.5));
        assert_eq!(anchor_of(ALIGN_BOTTOM | ALIGN_RIGHT), Vec2::new(1.0, 1.0));
        assert_eq!(anchor_of(ALIGN_TOP), Vec2::new(0.5, 0.0));
    }

    #[test]
    fn a_url_picks_its_plugin_by_scheme() {
        assert_eq!(kind_for_uri("rtmp://host/live/a"), "rtmp/source");
        assert_eq!(kind_for_uri("srt://host:1234"), "srt/source");
        assert_eq!(kind_for_uri("https://host/live.m3u8"), "hls/source");
        assert_eq!(kind_for_uri("/home/ada/clip.mp4"), "file/source");
        assert_eq!(kind_for_uri("C:\\clips\\ad.mp4"), "file/source");
    }

    #[test]
    fn an_obs_colour_is_read_back_to_front() {
        // OBS stores 0xAABBGGRR, so pure red is 0xff0000ff.
        assert_eq!(obs_colour(0xff0000ff), "#ff0000");
        assert_eq!(obs_colour(0xff00ff00), "#00ff00");
        assert_eq!(obs_colour(0xffffffff), "#ffffff");
    }

    #[test]
    fn blend_names_carry_across_and_anything_else_is_normal() {
        assert_eq!(blend_of(Some("multiply")), Blend::Multiply);
        assert_eq!(blend_of(Some("additive")), Blend::Add);
        assert_eq!(blend_of(None), Blend::Normal);
        assert_eq!(blend_of(Some("nonsense")), Blend::Normal);
    }
}
