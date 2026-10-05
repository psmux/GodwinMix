//! The station's three brand colours, and where its media library is: the
//! two things a template needs from the mixer's config.
//!
//! Set once at start, before any source is built, and read when a template
//! is checked. A process holds one show, so one of each is right.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// `[graphics]` in the config: the colours every template's `accent`,
/// `text` and `panel` fields take when a source does not set them. Change
/// these three and the whole pack is the station's colours.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(default)]
pub struct BrandConfig {
    /// The station's strong colour: a bar, a stripe, a flash. `#rrggbb`.
    /// Empty for each template's own default.
    pub accent: String,
    /// The colour of words on a panel. Empty for each template's own.
    pub text: String,
    /// The colour of the panels behind the words. Empty for each
    /// template's own.
    pub panel: String,
    /// The folder the Graphics gallery keeps its items in. Empty for a
    /// folder called `graphics` inside the media library.
    pub gallery: String,
}

impl BrandConfig {
    /// The brand's value for field `name`, when it is one of the three and set.
    pub fn get(&self, name: &str) -> Option<&str> {
        let v = match name {
            "accent" => &self.accent,
            "text" => &self.text,
            "panel" => &self.panel,
            _ => return None,
        };
        Some(v.trim()).filter(|v| !v.is_empty())
    }

    /// Each colour set is one a template can draw.
    pub fn check(&self) -> anyhow::Result<()> {
        for name in super::template::BRAND {
            if let Some(v) = self.get(name) {
                crate::plugin::kinds::text::style::colour(v).map_err(|e| anyhow::anyhow!("graphics.{name}: {e}"))?;
            }
        }
        Ok(())
    }
}

static BRAND: RwLock<Option<BrandConfig>> = RwLock::new(None);
static LIBRARY: RwLock<Option<PathBuf>> = RwLock::new(None);
static GALLERY: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Take the brand and the media folder from the config. Called at start.
pub fn configure(media_dir: &str, brand: &BrandConfig) {
    let dir = PathBuf::from(media_dir);
    let dir = dir.canonicalize().unwrap_or(dir);
    let gallery = match brand.gallery.trim() {
        "" => dir.join("graphics"),
        set => PathBuf::from(set),
    };
    *GALLERY.write() = Some(gallery);
    *LIBRARY.write() = Some(dir);
    let brand = match brand.check() {
        Ok(()) => brand.clone(),
        Err(e) => {
            tracing::warn!(error = %e, "the [graphics] brand colours are not colours; every template uses its own until they are fixed");
            BrandConfig::default()
        }
    };
    *BRAND.write() = Some(brand);
}

/// The brand colours in force.
pub fn brand() -> BrandConfig {
    BRAND.read().clone().unwrap_or_default()
}

/// The media folder a `template:` name is looked for in, if the mixer set
/// one. An embedding program that never called `configure` has none, and
/// only the pack and absolute paths resolve.
pub fn library() -> Option<PathBuf> {
    LIBRARY.read().clone()
}

/// The gallery's folder: `[graphics] gallery`, or `graphics` inside the
/// media library, or `graphics` where the mixer runs when neither was set.
pub fn gallery() -> PathBuf {
    GALLERY.read().clone().unwrap_or_else(|| PathBuf::from("graphics"))
}
