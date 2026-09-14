//! What a preset says about itself, and where one is found.
//!
//! A preset is a plugin of kind `preset` (06 section 5): a manifest naming
//! other plugins, a config, a UI layout, a theme and the scenes. Everything in
//! this file is reading and resolving; the checking is in `plan`, the writing
//! in `apply`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

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
    "welcome",
];

/// The UI slots a layout may fill (05 section 3).
pub const SLOTS: &[&str] = &["header", "main", "sidebar", "strip", "footer", "modal"];

/// The gallery tile modes (05 section 3b), cheapest last.
pub const GALLERY_MODES: &[&str] = &["live", "snapshot", "icon", "label"];

/// The themes the first party UI carries in `ui/themes/`.
pub const BUILT_IN_THEMES: &[&str] = &["dark", "light", "high-contrast", "system"];

/// A plugin manifest, read far enough to work with a preset. The full parser
/// belongs to the plugin loader.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Manifest {
    pub plugin: PluginBlock,
    #[serde(default)]
    pub provides: Vec<Provide>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginBlock {
    pub name: String,
    pub version: String,
    pub api: u32,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub platforms: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub placements: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Provide {
    pub kind: String,
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<PresetBlock>,
}

/// The `[provides.preset]` table of 03 section 4 and 06 section 5.
#[derive(Debug, Clone, Deserialize, Serialize)]
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
    /// What the gallery starts as for this preset's audience: `live`,
    /// `snapshot`, `icon` or `label`. Absent means "ask the machine".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    /// A stylesheet inside the preset, served as the preset's own theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme_css: Option<String>,
    /// The three things the person does after applying it. The welcome panel
    /// shows these, in this order, and the README repeats them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub steps: Vec<String>,
}

/// One plugin a preset names, split into the parts that matter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginSpec {
    pub name: String,
    pub range: String,
}

impl PluginSpec {
    /// `ndi@^1` becomes `{ name: "ndi", range: "^1" }`. A bare name is `*`.
    pub fn parse(text: &str) -> Self {
        match text.split_once('@') {
            Some((name, range)) => Self {
                name: name.trim().to_string(),
                range: range.trim().to_string(),
            },
            None => Self {
                name: text.trim().to_string(),
                range: "*".into(),
            },
        }
    }
}

/// A preset that has been found: its manifest, and where to read its files.
#[derive(Debug, Clone)]
pub struct Preset {
    pub name: String,
    /// The directory it came from, or `None` when it came out of the binary.
    pub dir: Option<PathBuf>,
    pub manifest: Manifest,
}

impl Preset {
    /// The `[provides.preset]` table, or an error naming what is missing.
    pub fn block(&self) -> Result<&PresetBlock> {
        let provide = self
            .manifest
            .provides
            .iter()
            .find(|p| p.kind == "preset")
            .with_context(|| format!("{} has no [[provides]] of kind \"preset\"", self.name))?;
        provide.preset.as_ref().with_context(|| {
            format!(
                "{} provides a preset but has no [provides.preset] table",
                self.name
            )
        })
    }

    pub fn description(&self) -> &str {
        &self.manifest.plugin.description
    }

    /// Where this preset came from, for a person reading a plan.
    pub fn origin(&self) -> String {
        match &self.dir {
            Some(dir) => dir.display().to_string(),
            None => "built in".to_string(),
        }
    }

    /// One file out of the preset, by its path relative to the preset root.
    pub fn read(&self, relative: &str) -> Result<String> {
        match &self.dir {
            Some(dir) => {
                let path = dir.join(relative);
                std::fs::read_to_string(&path)
                    .with_context(|| format!("reading {}", path.display()))
            }
            None => super::embedded::file(&self.name, relative)
                .map(str::to_string)
                .with_context(|| format!("the built in preset {} has no {relative}", self.name)),
        }
    }

    pub fn has(&self, relative: &str) -> bool {
        match &self.dir {
            Some(dir) => dir.join(relative).exists(),
            None => {
                super::embedded::file(&self.name, relative).is_some()
                    || super::embedded::entries(&self.name, relative)
                        .next()
                        .is_some()
            }
        }
    }

    /// Every `.json` file directly inside a directory of the preset, sorted by
    /// name so two runs produce the same collection in the same order.
    pub fn json_files(&self, relative: &str) -> Result<Vec<(String, String)>> {
        let mut found = Vec::new();
        match &self.dir {
            Some(dir) => {
                let sub = dir.join(relative);
                let entries = std::fs::read_dir(&sub)
                    .with_context(|| format!("reading {}", sub.display()))?;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let name = path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    let body = std::fs::read_to_string(&path)
                        .with_context(|| format!("reading {}", path.display()))?;
                    found.push((name, body));
                }
            }
            None => {
                for (name, body) in super::embedded::entries(&self.name, relative) {
                    if name.ends_with(".json") {
                        found.push((name.to_string(), body.to_string()));
                    }
                }
            }
        }
        found.sort_by(|a, b| a.0.cmp(&b.0));
        Ok(found)
    }
}

/// Where the presets live in a source checkout.
pub fn directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("presets")
}

/// Read one official preset's manifest out of the source tree.
///
/// Kept for the checks in `scene::presets`, which run against the repository.
/// Everything at runtime goes through `resolve`.
pub fn manifest(name: &str) -> Result<Manifest> {
    let path = directory().join(name).join("gmx-plugin.toml");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "there is no preset called {name:?}: {} is missing",
            path.display()
        )
    })?;
    parse(&text).with_context(|| format!("reading {}", path.display()))
}

/// Parse a manifest from its text.
pub fn parse(text: &str) -> Result<Manifest> {
    toml::from_str(text).context("the manifest is not valid TOML")
}

/// The directories searched for a preset by name, in order.
///
/// A checkout's own `presets/` first, so an edit is picked up without an
/// install; then `presets/` beside the binary, which is what a release archive
/// unpacks to; then the operator's own under `~/.godwinmix/presets`. Anything
/// still not found comes out of the binary.
pub fn search_paths() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join("presets"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            dirs.push(parent.join("presets"));
            // A cargo target directory is three levels below the checkout.
            if let Some(root) = parent.parent().and_then(|p| p.parent()) {
                dirs.push(root.join("presets"));
            }
        }
    }
    if let Some(home) = home_dir() {
        dirs.push(home.join(".godwinmix").join("presets"));
    }
    dirs.dedup();
    dirs
}

pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Load a preset from a directory on disk.
pub fn load(dir: &Path) -> Result<Preset> {
    let path = dir.join("gmx-plugin.toml");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "{} is not a preset: there is no gmx-plugin.toml in it. \
             `gmx preset list` shows the ones this build knows",
            dir.display()
        )
    })?;
    let manifest = parse(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(Preset {
        name: manifest.plugin.name.clone(),
        dir: Some(dir.to_path_buf()),
        manifest,
    })
}

/// Find a preset by name, or load one from a path.
///
/// Anything that looks like a path (it has a separator, or a directory of that
/// name exists here) is loaded from disk. Everything else is a name, searched
/// for in `search_paths` and then in the binary.
pub fn resolve(name_or_path: &str) -> Result<Preset> {
    let looks_like_path = name_or_path.contains('/')
        || name_or_path.contains('\\')
        || name_or_path.starts_with('.')
        || Path::new(name_or_path).join("gmx-plugin.toml").is_file();
    if looks_like_path {
        return load(Path::new(name_or_path));
    }
    for dir in search_paths() {
        let candidate = dir.join(name_or_path);
        if candidate.join("gmx-plugin.toml").is_file() {
            return load(&candidate);
        }
    }
    if let Some(text) = super::embedded::file(name_or_path, "gmx-plugin.toml") {
        let manifest = parse(text)
            .with_context(|| format!("the built in preset {name_or_path} has a bad manifest"))?;
        return Ok(Preset {
            name: name_or_path.to_string(),
            dir: None,
            manifest,
        });
    }
    let known = list()
        .into_iter()
        .map(|p| p.name)
        .collect::<Vec<_>>()
        .join(", ");
    anyhow::bail!(
        "there is no preset called {name_or_path:?}. \
         This build knows: {known}. A directory works too: `gmx preset apply ./my-preset`"
    )
}

/// Every preset this machine can apply: the built in six, plus anything in a
/// search path. A directory shadows the built in one of the same name.
pub fn list() -> Vec<Preset> {
    let mut found: Vec<Preset> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for dir in search_paths() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.join("gmx-plugin.toml").is_file() {
                continue;
            }
            let Ok(preset) = load(&path) else { continue };
            if seen.contains(&preset.name) {
                continue;
            }
            seen.push(preset.name.clone());
            found.push(preset);
        }
    }
    for name in NAMES {
        if seen.iter().any(|s| s == name) {
            continue;
        }
        let Some(text) = super::embedded::file(name, "gmx-plugin.toml") else {
            continue;
        };
        let Ok(manifest) = parse(text) else { continue };
        found.push(Preset {
            name: (*name).to_string(),
            dir: None,
            manifest,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plugin_spec_splits_at_the_at_sign() {
        assert_eq!(
            PluginSpec::parse("ndi@^1"),
            PluginSpec {
                name: "ndi".into(),
                range: "^1".into()
            }
        );
        assert_eq!(
            PluginSpec::parse("ndi"),
            PluginSpec {
                name: "ndi".into(),
                range: "*".into()
            }
        );
    }

    #[test]
    fn every_official_preset_is_in_the_binary() {
        for name in NAMES {
            let preset = resolve(name).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert_eq!(&preset.name, name);
            let block = preset.block().unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert!(!block.config.is_empty());
        }
    }

    #[test]
    fn a_preset_reads_its_own_files_whether_it_came_from_disk_or_the_binary() {
        let from_binary = Preset {
            name: "church".into(),
            dir: None,
            manifest: manifest("church").unwrap(),
        };
        let from_disk = load(&directory().join("church")).unwrap();
        let block = from_disk.block().unwrap().clone();
        assert_eq!(
            from_binary.read(&block.config).unwrap(),
            from_disk.read(&block.config).unwrap()
        );
        assert_eq!(
            from_binary.json_files(&block.scenes).unwrap().len(),
            from_disk.json_files(&block.scenes).unwrap().len()
        );
    }

    #[test]
    fn a_name_nobody_has_says_what_the_names_are() {
        let e = resolve("cathedral").unwrap_err().to_string();
        assert!(e.contains("church"), "{e}");
        assert!(e.contains("gmx preset apply ./"), "{e}");
    }
}
