//! The official presets, checked.
//!
//! A preset is data under `presets/<name>/`, not code: a manifest, a config, a
//! UI layout, a theme and the scenes. `gmx preset apply` lands in a later wave.
//! What is here is the reading and the checking, so that a preset with a typo
//! in it fails the build rather than a volunteer's Sunday.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// The six presets that ship with GodwinMix (06 section 5).
pub const NAMES: &[&str] = &[
    "default",
    "church",
    "classroom",
    "esports",
    "headless-agent",
    "broadcast",
];

/// The panels the first party UI ships (05 section 3). A preset's layout may
/// also name a panel a plugin provides, which is prefixed with its plugin name.
pub const PANELS: &[&str] = &[
    "header",
    "multiview",
    "sources",
    "outputs",
    "media",
    "alerts",
    "scenes",
];

/// The UI slots a layout may fill (05 section 3).
pub const SLOTS: &[&str] = &["header", "main", "sidebar", "strip", "footer", "modal"];

/// A plugin manifest, read far enough to check a preset. The full parser
/// belongs to the plugin loader.
#[derive(Debug, Clone, Deserialize)]
pub struct Manifest {
    pub plugin: PluginBlock,
    #[serde(default)]
    pub provides: Vec<Provide>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PluginBlock {
    pub name: String,
    pub version: String,
    pub api: u32,
    pub description: String,
    #[serde(default)]
    pub license: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Provide {
    pub kind: String,
    pub id: String,
    #[serde(default)]
    pub preset: Option<PresetBlock>,
}

/// The `[provides.preset]` table of 03 section 4 and 06 section 5.
#[derive(Debug, Clone, Deserialize)]
pub struct PresetBlock {
    /// Plugins by name and semver range, for example `ndi@^1`.
    pub plugins: Vec<String>,
    pub config: String,
    pub layout: String,
    /// `web`, `none`, or a surface plugin's name.
    pub surface: String,
    pub theme: String,
    /// The directory of scene documents.
    pub scenes: String,
}

/// Where the presets live, relative to the repository root.
pub fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("presets")
}

/// Read one preset's manifest.
pub fn manifest(name: &str) -> Result<Manifest> {
    let path = directory().join(name).join("gmx-plugin.toml");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "there is no preset called {name:?}: {} is missing",
            path.display()
        )
    })?;
    toml::from_str(&text).with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::scene::document::Collection;
    use crate::scene::{layout, validate};
    use std::collections::BTreeMap;

    fn preset_dir(name: &str) -> PathBuf {
        directory().join(name)
    }

    #[test]
    fn every_official_preset_has_a_manifest_that_says_what_it_is() {
        for name in NAMES {
            let manifest = manifest(name).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert_eq!(
                &manifest.plugin.name, name,
                "a preset's name is its directory"
            );
            assert_eq!(manifest.plugin.api, 1);
            assert!(!manifest.plugin.version.is_empty());
            assert!(
                manifest.plugin.description.len() > 40,
                "{name}: the description is what somebody browsing the index reads"
            );
            let preset = manifest
                .provides
                .iter()
                .find(|p| p.kind == "preset")
                .unwrap_or_else(|| panic!("{name} provides no preset"));
            assert_eq!(&preset.id, name);
            let block = preset.preset.as_ref().expect("a [provides.preset] table");
            assert!(!block.theme.is_empty(), "{name} names no theme");
            for path in [&block.config, &block.layout, &block.scenes] {
                assert!(
                    preset_dir(name).join(path).exists(),
                    "{name}: {path} is missing"
                );
            }
            for plugin in &block.plugins {
                assert!(
                    plugin.contains('@'),
                    "{name}: {plugin:?} needs a version range"
                );
            }
        }
    }

    #[test]
    fn every_preset_config_is_a_configuration_this_build_can_load() {
        for name in NAMES {
            let path = preset_dir(name).join("config/godwinmix.toml");
            let config = Config::load(&path).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert!(!config.sources.is_empty(), "{name} has no sources");
            assert!(!config.outputs.is_empty(), "{name} has no outputs");
            // Ids are slugs an operator types, even in a file nobody wrote by
            // hand.
            for source in &config.sources {
                assert!(
                    source
                        .id
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                    "{name}: the source id {:?} is not a slug",
                    source.id
                );
            }
        }
    }

    #[test]
    fn every_preset_config_names_a_type_and_params_for_every_source() {
        // The `type` and `params` form of 03 section 3. Today's loader ignores
        // both, which is why this reads the file rather than the struct.
        for name in NAMES {
            let text =
                std::fs::read_to_string(preset_dir(name).join("config/godwinmix.toml")).unwrap();
            let raw: toml::Value = toml::from_str(&text).unwrap();
            for source in raw["sources"].as_array().unwrap() {
                let id = source["id"].as_str().unwrap();
                let kind = source["type"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{name}: {id} has no type"));
                assert!(
                    kind.contains('/'),
                    "{name}: {id} has type {kind:?}, which is not plugin/provide"
                );
                assert!(
                    source.get("params").is_some(),
                    "{name}: {id} has no params table"
                );
            }
            for output in raw["outputs"].as_array().unwrap() {
                let id = output["id"].as_str().unwrap();
                assert!(
                    output["type"].as_str().is_some_and(|k| k.contains('/')),
                    "{name}: the output {id} has no plugin qualified type"
                );
            }
        }
    }

    #[test]
    fn every_preset_ui_layout_names_real_slots_and_panels() {
        for name in NAMES {
            let text =
                std::fs::read_to_string(preset_dir(name).join("config/layout.json")).unwrap();
            let layout: BTreeMap<String, Vec<String>> =
                serde_json::from_str(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(!layout.is_empty(), "{name} has an empty layout");
            for (slot, panels) in &layout {
                assert!(
                    SLOTS.contains(&slot.as_str()),
                    "{name}: {slot:?} is not a UI slot"
                );
                for panel in panels {
                    // A plugin's panel is prefixed with its plugin name.
                    let known = PANELS.contains(&panel.as_str()) || panel.contains('/');
                    assert!(known, "{name}: {panel:?} is not a panel");
                }
            }
        }
    }

    #[test]
    fn every_preset_scene_is_a_document_that_parses_validates_and_applies() {
        for name in NAMES {
            let scenes = preset_dir(name).join("scenes");
            let mut found = 0;
            for entry in std::fs::read_dir(&scenes).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                found += 1;
                let text = std::fs::read_to_string(&path).unwrap();
                let doc = Collection::from_json(&text)
                    .unwrap_or_else(|e| panic!("{name}/{}: {e:#}", path.display()));
                let findings = validate::collection(&doc);
                assert!(
                    !validate::has_errors(&findings),
                    "{name}/{}: {findings:#?}",
                    path.display()
                );
                // Every scene a preset ships is a layout, so it has to resolve
                // against that preset's own sources.
                let values = values_from_config(name, &doc);
                let scene = layout::apply(&doc, &values, doc.canvas)
                    .unwrap_or_else(|e| panic!("{name}/{}: {e:#}", path.display()));
                let text = serde_json::to_string(&scene).unwrap();
                assert!(
                    !text.contains("{{"),
                    "{name}/{}: a binding was left over",
                    path.display()
                );
            }
            assert!(found > 0, "{name} ships no scenes");
        }
    }

    /// Fill a layout's source slots with the preset's own sources, in order.
    fn values_from_config(name: &str, doc: &Collection) -> layout::Values {
        let config = Config::load(&preset_dir(name).join("config/godwinmix.toml")).unwrap();
        let mut ids = config.sources.iter().map(|s| s.id.clone()).cycle();
        let mut values = layout::Values::new();
        let properties = doc
            .params
            .get("properties")
            .and_then(serde_json::Value::as_object);
        for (key, schema) in properties.into_iter().flatten() {
            match schema.get("x-gmx-kind").and_then(serde_json::Value::as_str) {
                Some("source") => {
                    values.insert(key.clone(), serde_json::Value::from(ids.next().unwrap()));
                }
                Some("graphic") => {
                    values.insert(key.clone(), serde_json::Value::from("lowerthird/graphic"));
                }
                _ => {}
            }
        }
        values
    }

    #[test]
    fn no_two_presets_share_a_node_id() {
        // Two presets applied to the same core must not collide, and a copied
        // directory with the ids left alone is the way that happens.
        let mut seen: BTreeMap<String, String> = BTreeMap::new();
        for name in NAMES {
            for entry in std::fs::read_dir(preset_dir(name).join("scenes")).unwrap() {
                let path = entry.unwrap().path();
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(doc) = Collection::from_json(&text) else {
                    continue;
                };
                let mut ids = vec![doc.id.to_string()];
                for scene in &doc.scenes {
                    ids.push(scene.id.to_string());
                    ids.extend(scene.walk().iter().map(|i| i.id.to_string()));
                }
                for id in ids {
                    let where_now =
                        format!("{name}/{}", path.file_name().unwrap().to_string_lossy());
                    if let Some(before) = seen.insert(id.clone(), where_now.clone()) {
                        panic!("{id} is in both {before} and {where_now}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_preset_readme_answers_the_four_questions() {
        for name in NAMES {
            let text = std::fs::read_to_string(preset_dir(name).join("README.md")).unwrap();
            for heading in [
                "What it gives you",
                "What you need",
                "Three steps",
                "When it does not work",
            ] {
                assert!(
                    text.contains(heading),
                    "{name}'s README has no {heading:?} section"
                );
            }
            assert!(
                text.contains("gmx preset apply"),
                "{name}'s README never says the command that installs it"
            );
        }
    }

    #[test]
    fn the_presets_readme_explains_how_to_copy_one() {
        let text = std::fs::read_to_string(directory().join("README.md")).unwrap();
        assert!(text.contains("cp -r presets/"), "no copy instruction");
        for name in NAMES {
            assert!(
                text.contains(&format!("`{name}`")),
                "{name} is not in the table"
            );
        }
    }
}
