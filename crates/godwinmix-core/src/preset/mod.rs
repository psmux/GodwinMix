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
mod merge;
pub mod plan;
pub mod save;
pub mod step;

pub use apply::{apply_named, Applied};
pub use manifest::{load, resolve, Manifest, Preset, PresetBlock, NAMES};
pub use plan::{Options, Plan};
pub use step::Step;

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
    /// The things to do after applying it, as sentences.
    pub steps: Vec<String>,
    /// The same steps with what each one does, for a page to draw as a
    /// checklist. A step with no `does` is prose.
    pub checklist: Vec<Step>,
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
                steps: step::texts(&block.steps),
                checklist: block.steps.clone(),
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
            assert_eq!(row.steps, step::texts(&row.checklist), "{name}: the sentences drifted");
            for step in &row.steps {
                assert!(step.len() > 10, "{name}: {step:?} is not a step");
            }
        }
    }

    /// The person reading these is already in the mixer, looking at the page.
    /// A step that sends them to a file, a terminal or an address is the
    /// defect this structure replaced.
    #[test]
    fn every_official_step_does_something_and_names_no_file_command_or_address() {
        for row in list().into_iter().filter(|l| l.official) {
            let preset = resolve(&row.name).unwrap();
            let config: toml::Value =
                toml::from_str(&preset.read(&preset.block().unwrap().config).unwrap()).unwrap();
            for step in &row.checklist {
                let text = &step.text;
                for banned in ["`", "http", ".toml", "gmx ", "[[", "localhost", "config"] {
                    assert!(!text.contains(banned), "{}: {text:?} names {banned:?}", row.name);
                }
                assert!(step.does.is_some(), "{}: {text:?} does nothing", row.name);
                assert_eq!(step.problem(), None, "{}", row.name);
                assert_target_exists(&row.name, step, &config);
            }
        }
    }

    /// An `add-key` names an output the preset makes, and a `take` a source
    /// the preset makes, so the button has something to open.
    fn assert_target_exists(name: &str, step: &Step, config: &toml::Value) {
        let target = step.target.as_deref().unwrap_or_default();
        let ids = |table: &str| -> Vec<String> {
            config
                .get(table)
                .and_then(|v| v.as_array())
                .map(|a| a.iter().filter_map(|t| t.get("id")?.as_str().map(String::from)).collect())
                .unwrap_or_default()
        };
        match step.does.as_deref() {
            Some("add-key") => assert!(ids("outputs").iter().any(|i| i == target), "{name}: no output {target}"),
            Some("take") => {
                let scenes = resolve(name).unwrap();
                let block = scenes.block().unwrap();
                let docs = scenes.json_files(&block.scenes).unwrap_or_default();
                let is_scene = docs.iter().any(|(_, doc)| doc.contains(&format!("\"name\": \"{target}\"")));
                assert!(ids("sources").iter().any(|i| i == target) || is_scene, "{name}: nothing called {target}");
            }
            _ => {}
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
