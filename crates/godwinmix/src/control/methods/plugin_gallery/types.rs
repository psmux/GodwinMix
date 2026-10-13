//! What `plugin.gallery` takes and answers.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// `plugin.gallery`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct GalleryRequest {
    /// Fetch every marketplace now, however fresh its copy. Without it a copy
    /// younger than ten minutes is read as it is.
    #[serde(default)]
    pub refresh: bool,
}

/// What `plugin.gallery` answers with.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Gallery {
    /// This machine, as a platform triple: `windows-x86_64`.
    pub platform: String,
    /// The marketplaces read, and how fresh each copy is.
    pub marketplaces: Vec<GalleryMarketplace>,
    /// Every plugin listed, once each, highest tier first.
    pub plugins: Vec<GalleryEntry>,
    /// True when a marketplace was due and could not be reached, so what is
    /// shown is the copy from before.
    pub offline: bool,
}

/// One marketplace the gallery read.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GalleryMarketplace {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub title: String,
    /// What was added: `owner/repo`, a URL or a path.
    pub source: String,
    /// When the copy was fetched, seconds since the epoch. 0 when there is none.
    pub fetched: u64,
    /// Why it was not fetched just now, when it was due.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// One plugin, as a gallery card and its detail view show it.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct GalleryEntry {
    pub name: String,
    /// The human name. The name when the listing has none.
    pub title: String,
    pub description: String,
    /// custom, bronze, silver or gold.
    pub tier: String,
    pub kinds: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub repository: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub homepage: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub author: String,
    /// Picture URLs, loaded by the browser from where they are.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub icon: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub thumbnail: String,
    #[serde(default)]
    pub screenshots: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stars: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<u64>,
    /// When it last changed, RFC 3339.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub updated: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// It ships with GodwinMix and installs with no download.
    pub bundled: bool,
    /// The newest listed version this mixer can run, else the newest listed.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// Every version listed, oldest first.
    #[serde(default)]
    pub versions: Vec<String>,
    /// Whether that version's assets were signed by the marketplace's CI.
    pub signed: bool,
    /// What to pass to `plugin.add` as `source`.
    pub install: String,
    pub marketplace: String,
    pub installed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installed_version: Option<String>,
    /// Installed, and the listing has a newer version.
    pub update: bool,
    /// Whether it runs on this machine at this mixer's protocol level.
    pub runs_here: bool,
    /// The card's word for that: "Runs here", "Not for Windows".
    pub fit: String,
    /// The sentence behind it.
    pub fit_detail: String,
}
