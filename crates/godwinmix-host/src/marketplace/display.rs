//! What a gallery shows about a listing and an install does not need.
//!
//! The community index's workflow fills these in from each plugin's own
//! repository: the README's first paragraph, the pictures under `gallery/`,
//! the stars and the release download counts. Every one is optional, so an
//! index written before they existed parses exactly as it did, and a
//! hand written marketplace can leave them all out.
//!
//! The pictures are URLs the browser loads itself. The core never fetches
//! one, so a gallery costs the mixer one small JSON document and nothing else.

use serde::{Deserialize, Serialize};

/// The display fields of one listing, flattened into it on the wire.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Display {
    /// A human name, "NDI" for `ndi`. The name is the fallback.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// A square picture, a URL.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    /// The card's picture, a URL. Wider than tall.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thumbnail: String,
    /// Pictures of it working, URLs, for the detail view. Anything an
    /// `<img>` shows, so an animated GIF or WebP is here and a video is not.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub screenshots: Vec<String>,
    /// Every picture and clip in the plugin's `gallery/` folder, in its
    /// order, each with what it is. An index written before this field
    /// existed has none, and a gallery reads `screenshots` instead.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media: Vec<Media>,
    /// The first animation or video in `media`, which a card plays on hover.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub preview: String,
    /// The index put it in the Featured row.
    #[serde(default, skip_serializing_if = "is_false")]
    pub featured: bool,
    /// The oldest GodwinMix that runs it, `0.3.10`. A listing with one is
    /// only in `index2.json`, which older mixers never read.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mixer: String,
    /// The repository's GitHub stars when the index last looked.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<u64>,
    /// How many times its release assets were downloaded, every release
    /// added together.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    /// When it last changed, RFC 3339.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub updated: String,
    /// Who makes it.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    /// Words to search by, from the repository's topics.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// It ships with GodwinMix, so installing it needs no download.
    #[serde(default, skip_serializing_if = "is_false")]
    pub bundled: bool,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// One picture or clip from a plugin's `gallery/` folder.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Media {
    pub url: String,
    /// `image`, `animation` or `video`. Anything else is read as an image.
    #[serde(rename = "type", default)]
    pub kind: String,
}

impl Media {
    /// The kind of one URL, from its extension, for an index that only gave
    /// `screenshots`: a `.gif` moves, an `.mp4` or `.webm` is a video.
    pub fn guess(url: &str) -> Self {
        let path = url.split(['?', '#']).next().unwrap_or("").to_ascii_lowercase();
        let kind = if path.ends_with(".gif") {
            "animation"
        } else if path.ends_with(".mp4") || path.ends_with(".webm") {
            "video"
        } else {
            "image"
        };
        Self { url: url.to_string(), kind: kind.to_string() }
    }
}
