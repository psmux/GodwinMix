//! Presets: a name for a working setup, and the four files that make it one.
//!
//! Principle 10 says presets are products. A preset bundles the plugins it
//! needs, a configuration, a UI layout, a theme and the scenes under one name,
//! and one command puts the whole of it on a machine:
//!
//! ```text
//! gmx preset apply church
//! ```
//!
//! The parts:
//!
//! * `manifest` reads `gmx-plugin.toml` and finds a preset by name or by path,
//!   on disk or out of the binary.
//! * `plan` works out what applying it would do, and refuses a preset that is
//!   broken (a missing file, a config this build cannot load, a scene that does
//!   not validate). A plugin that is not installed is reported, never fatal.
//! * `apply` writes the config, the scene collection and the surface defaults.
//! * `save` turns a working machine back into a preset somebody else can apply.
//! * `embedded` is the six official presets, compiled in, so the volunteer who
//!   downloaded one file can still run `gmx preset apply church`.
//!
//! `docs/reference/presets.md` is the manifest keys and the merge rules;
//! `docs/how-to/make-a-preset.md` is the walk through.

pub mod apply;
pub mod embedded;
pub mod manifest;
pub mod plan;
pub mod save;

pub use apply::{apply_named, Applied};
pub use manifest::{load, resolve, Manifest, Preset, PresetBlock, NAMES};
pub use plan::{Options, Plan};

/// One row of `gmx preset list`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Listed {
    pub name: String,
    pub version: String,
    pub description: String,
    /// The directory it came from, or "built in".
    pub origin: String,
    pub theme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    pub surface: String,
    /// The plugins it names, as written.
    pub plugins: Vec<String>,
    /// The three things to do after applying it.
    pub steps: Vec<String>,
    /// True when it is one of the six that ship with GodwinMix.
    pub official: bool,
}

/// Every preset this machine can apply.
pub fn list() -> Vec<Listed> {
    manifest::list()
        .into_iter()
        .filter_map(|preset| {
            let block = preset.block().ok()?;
            Some(Listed {
                name: preset.name.clone(),
                version: preset.manifest.plugin.version.clone(),
                description: preset.manifest.plugin.description.clone(),
                origin: preset.origin(),
                theme: block.theme.clone(),
                gallery: block.gallery.clone(),
                surface: block.surface.clone(),
                plugins: block.plugins.clone(),
                steps: block.steps.clone(),
                official: NAMES.contains(&preset.name.as_str()),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_six_official_presets_are_listed_with_their_steps() {
        let listed = list();
        for name in NAMES {
            let row = listed
                .iter()
                .find(|l| &l.name == name)
                .unwrap_or_else(|| panic!("{name} is not listed"));
            assert!(row.official);
            assert!(!row.theme.is_empty(), "{name} names no theme");
            assert_eq!(row.steps.len(), 3, "{name} has {} steps, wanted three", row.steps.len());
            for step in &row.steps {
                assert!(step.len() > 10, "{name}: {step:?} is not a step");
            }
        }
    }

    #[test]
    fn every_official_preset_names_a_gallery_mode() {
        for row in list().into_iter().filter(|l| l.official) {
            let mode = row.gallery.unwrap_or_default();
            assert!(
                manifest::GALLERY_MODES.contains(&mode.as_str()),
                "{}: gallery = {mode:?}",
                row.name
            );
        }
    }
}
