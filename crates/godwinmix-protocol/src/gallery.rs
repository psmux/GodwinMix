//! The graphics gallery: every designed asset the mixer can put on air, in
//! one list, whoever made it.
//!
//! An item is an SVG template, a picture, a clip, an HTML or OGraf page, a
//! ticker or text preset, or a set (a background with a foreground and the
//! presenter layout's settings). Each lives in a folder of its own beside
//! the media library, described by a `graphic.toml`; the pack templates and
//! the shipped starter designs are listed beside them, read only.
//!
//! The request types are forgiving on purpose. A small model calls these as
//! often as a large one does, so a kind or a zone is read from the common
//! ways of writing it, and a tag list may be one string with commas in it.

#[path = "gallery_actions.rs"]
mod actions;
#[path = "gallery_requests.rs"]
mod requests;
#[path = "gallery_zone.rs"]
mod zone;
pub use actions::*;
pub use requests::*;
pub use zone::Zone;

use crate::graphics::TemplateField;
use zone::normal;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// What an item is, which decides how it is drawn and how it is added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GalleryKind {
    /// An SVG with `{{fields}}`, drawn by the mixer (`template:`).
    Template,
    /// A still picture: PNG, WebP, JPEG, or an SVG with no fields.
    Image,
    /// A moving picture: WebM, MOV or MP4, with alpha or without.
    Clip,
    /// A web page, drawn by the browser source.
    Html,
    /// An OGraf package (`graphic.ograf.json` beside its web component).
    Ograf,
    /// A crawl of words, kept as the `ticker:` source it adds.
    Ticker,
    /// Words in a box, kept as the `text:` source it adds.
    Text,
    /// A virtual set: a background, a foreground, and the layout settings
    /// that `scene.create_from` turns into a new scene around a presenter.
    Set,
    /// A stinger, a luma wipe or a shader played by a take. Listed and
    /// previewed here; played by the transition methods, not placed.
    Transition,
    /// An overlay fired on demand: a light leak, bokeh, a glitch. Listed and
    /// previewed here; fired by the effect methods, not placed.
    Effect,
}

impl GalleryKind {
    pub const ALL: [GalleryKind; 10] = [
        Self::Template, Self::Image, Self::Clip, Self::Html, Self::Ograf, Self::Ticker, Self::Text, Self::Set,
        Self::Transition, Self::Effect,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Template => "template",
            Self::Image => "image",
            Self::Clip => "clip",
            Self::Html => "html",
            Self::Ograf => "ograf",
            Self::Ticker => "ticker",
            Self::Text => "text",
            Self::Set => "set",
            Self::Transition => "transition",
            Self::Effect => "effect",
        }
    }

    /// True for the kinds a take or a trigger plays rather than a scene holds.
    pub fn is_played(self) -> bool {
        matches!(self, Self::Transition | Self::Effect)
    }

    /// Read a kind the way people and models write it: `svg` is a template,
    /// `png` and `picture` are an image, `video` and `webm` are a clip,
    /// `web` and `page` are html, `crawl` is a ticker, `virtual set` is a set.
    pub fn parse(text: &str) -> Option<Self> {
        let t = normal(text);
        Some(match t.as_str() {
            "template" | "svg" | "svg-template" | "graphic-template" => Self::Template,
            "image" | "picture" | "png" | "webp" | "jpeg" | "jpg" | "still" | "photo" | "logo" => Self::Image,
            "clip" | "video" | "movie" | "webm" | "mov" | "mp4" | "animation" => Self::Clip,
            "html" | "web" | "page" | "webpage" | "browser" | "html-graphic" => Self::Html,
            "ograf" | "o-graf" | "ograf-graphic" => Self::Ograf,
            "ticker" | "crawl" | "scroller" | "credits" | "news-ticker" => Self::Ticker,
            "text" | "words" | "caption" | "label" => Self::Text,
            "set" | "virtual-set" | "vset" | "studio-set" => Self::Set,
            "transition" | "stinger" | "wipe" | "luma" | "luma-wipe" | "shader" => Self::Transition,
            "effect" | "fx" | "light-leak" | "bokeh" | "glitch" => Self::Effect,
            _ => return None,
        })
    }
}

/// Where an item came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// Built into the mixer: the template pack and the starter designs.
    /// Read only; duplicate one to change it.
    Shipped,
    /// Saved by an AI agent or a script through `gallery.save`.
    #[default]
    Agent,
    /// A file a person added: an upload, an import, a template saved in the
    /// media library.
    Uploaded,
}

/// One item, as `gallery.list` describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryItem {
    /// The slug every other gallery method takes: `storm-lower-third`.
    pub id: String,
    pub name: String,
    pub kind: GalleryKind,
    pub zone: Zone,
    /// Whether it moves by itself: a clip, a page, a ticker.
    pub moves: bool,
    /// Whether the picture under it shows through anywhere.
    pub transparent: bool,
    pub origin: Origin,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// The fields a template or OGraf graphic has, with their defaults.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<TemplateField>,
    /// The values this item fills its fields with, over the defaults.
    #[serde(default, skip_serializing_if = "Map::is_empty")]
    pub values: Map<String, Value>,
    /// The address `source.add` takes for it, when it is one source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    /// Who or what saved it, in its own words: `claude-code`, `opencode`.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub made_by: String,
    /// When it was saved, as RFC 3339, when known.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub saved: String,
    /// A file of the item a page plays as its moving preview, served at
    /// `/api/v1/gallery/{id}/files/{moving}`: a clip itself, or the item's
    /// own `preview.webm`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moving: Option<String>,
    /// The sources on this mixer drawing it now.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placed: Vec<String>,
}

/// The answer to `gallery.list`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GalleryList {
    pub items: Vec<GalleryItem>,
    /// The folder saved items live in, on the mixer.
    pub dir: String,
    /// How many items match before `limit` cut the list.
    pub total: usize,
    /// Folders that look like items and would not read, and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<String>,
}
