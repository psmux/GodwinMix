//! Carrying a plan out: the config file, the scene collection, the surface.
//!
//! Three files are written and nothing else. The config file gets the preset's
//! values where the operator had none. The collection file gets the preset's
//! scenes. The runtime store gets a `[ui]` section saying which layout, theme
//! and gallery mode the surface starts with.
//!
//! A config file that did not exist is copied from the preset verbatim, with
//! its comments, because those comments are the preset's documentation and the
//! volunteer in 09 reads them before anything else. A config file that did
//! exist is merged key by key and the original is kept beside it as `.bak`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use super::manifest::Preset;
use super::plan::Plan;
use crate::config::{Config, UiDefaults};
use crate::scene::document::{Canvas, Collection, SCHEMA_VERSION};
use crate::scene::Id;

/// What was actually written.
#[derive(Debug, Clone, Serialize)]
pub struct Applied {
    pub preset: String,
    /// Every file this wrote, in the order it wrote them.
    pub wrote: Vec<PathBuf>,
    /// The original config, kept when one was overwritten.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub backup: Option<PathBuf>,
    pub ui: UiDefaults,
    /// The sources and outputs a running core should pick up now.
    pub sources: Vec<String>,
    pub outputs: Vec<String>,
    pub todo: Vec<String>,
    pub steps: Vec<String>,
}

/// Write the plan.
pub fn run(preset: &Preset, plan: &Plan) -> Result<Applied> {
    let block = preset.block()?;
    let preset_config = preset.read(&block.config)?;
    let backup = write_config(plan, &preset_config)?;
    let mut wrote = vec![plan.config_path.clone()];

    let merged = Config::load(&crate::config::path_in_force(&plan.config_path))
        .with_context(|| "the merged config does not load; the original is beside it as .bak")?;
    let scenes = super::plan::resolve_scenes(preset, block, &merged)?;
    write_scenes(&plan.scenes_path, &plan.name, &merged, scenes)?;
    wrote.push(plan.scenes_path.clone());

    let ui = UiDefaults {
        preset: Some(plan.name.clone()),
        theme: Some(plan.theme.clone()),
        gallery: plan.gallery.clone(),
        layout: plan.layout.clone(),
    };
    let store = write_ui(&plan.config_path, &ui)?;
    wrote.push(store);

    Ok(Applied {
        preset: plan.name.clone(),
        wrote,
        backup,
        ui,
        sources: plan
            .sources
            .iter()
            .filter(|s| !s.already_there)
            .map(|s| s.id.clone())
            .collect(),
        outputs: plan
            .outputs
            .iter()
            .filter(|o| !o.already_there)
            .map(|o| o.id.clone())
            .collect(),
        todo: plan.todo.clone(),
        steps: plan.steps.clone(),
    })
}

/// The config file. Returns the backup path when one was made.
fn write_config(plan: &Plan, preset_config: &str) -> Result<Option<PathBuf>> {
    let path = crate::config::path_in_force(&plan.config_path);
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("making {}", parent.display()))?;
        }
    }
    if !path.exists() {
        // Nothing to merge into: the preset's own file, comments and all.
        std::fs::write(&path, preset_config)
            .with_context(|| format!("writing {}", path.display()))?;
        return Ok(None);
    }
    let original =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let mut current: toml::Table = toml::from_str(&original)
        .with_context(|| format!("{} is not valid TOML", path.display()))?;
    let preset_table: toml::Table =
        toml::from_str(preset_config).context("the preset's config is not valid TOML")?;

    merge(&mut current, &preset_table, plan.force);
    if !plan.keep_sources {
        append_by_id(&mut current, &preset_table, "sources");
        append_by_id(&mut current, &preset_table, "outputs");
    }

    let backup = path.with_extension("toml.bak");
    std::fs::write(&backup, &original).with_context(|| format!("writing {}", backup.display()))?;
    let body = format!(
        "# Merged by `gmx preset apply {}`. The file as it was before is in {}.\n\
         # Comments from your own file are in that copy: this one is rewritten from\n\
         # the values, which is the price of merging two configurations.\n\n{}",
        plan.name,
        backup.file_name().unwrap_or_default().to_string_lossy(),
        toml::to_string_pretty(&current).context("writing the merged config")?
    );
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    Ok(Some(backup))
}

/// Preset values fill gaps; the operator's own win unless `force`.
fn merge(current: &mut toml::Table, preset: &toml::Table, force: bool) {
    for (key, value) in preset {
        if matches!(key.as_str(), "sources" | "outputs") {
            continue;
        }
        match (current.get_mut(key), value) {
            (Some(toml::Value::Table(mine)), toml::Value::Table(theirs)) => {
                merge(mine, theirs, force)
            }
            (Some(_), _) if !force => {}
            (_, _) => {
                current.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Append the preset's entries whose id is not already in the operator's list.
fn append_by_id(current: &mut toml::Table, preset: &toml::Table, key: &str) {
    let Some(incoming) = preset.get(key).and_then(toml::Value::as_array) else {
        return;
    };
    let mut list = current
        .get(key)
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let have: Vec<String> = list
        .iter()
        .filter_map(|v| v.get("id")?.as_str().map(str::to_string))
        .collect();
    for entry in incoming {
        let Some(id) = entry.get("id").and_then(toml::Value::as_str) else {
            continue;
        };
        if have.iter().any(|h| h == id) {
            continue;
        }
        list.push(entry.clone());
    }
    if !list.is_empty() {
        current.insert(key.to_string(), toml::Value::Array(list));
    }
}

/// The scene collection, as the tree projection 11 section 2 describes.
///
/// One collection holding every scene the preset ships, written whole. A
/// collection already there keeps its own scenes: the preset's are appended by
/// name, so applying a preset twice does not double them.
fn write_scenes(
    path: &Path,
    name: &str,
    config: &Config,
    scenes: Vec<crate::scene::Scene>,
) -> Result<()> {
    let canvas = Canvas {
        width: config.canvas.width.max(2) as u32,
        height: config.canvas.height.max(2) as u32,
        fps: config.canvas.fps.max(1) as u32,
    };
    let mut collection = match std::fs::read_to_string(path).ok() {
        Some(text) => Collection::from_json(&text)
            .with_context(|| format!("{} is not a scene collection", path.display()))?,
        None => Collection {
            schema_version: SCHEMA_VERSION,
            id: Id::new(),
            name: format!("{name} scenes"),
            canvas,
            params: crate::scene::document::empty_params(),
            scenes: Vec::new(),
            transitions: Vec::new(),
            assets: Default::default(),
        },
    };
    for scene in scenes {
        match collection.scenes.iter_mut().find(|s| s.name == scene.name) {
            Some(existing) => *existing = scene,
            None => collection.scenes.push(scene),
        }
    }
    let body = serde_json::to_string_pretty(&collection).context("writing the scenes")?;
    write_atomic(path, body.as_bytes())
}

/// The `[ui]` section of the runtime store: what a surface starts with.
fn write_ui(config_path: &Path, ui: &UiDefaults) -> Result<PathBuf> {
    let path = Config::runtime_store_path(&crate::config::path_in_force(config_path));
    let mut table: toml::Table = match std::fs::read_to_string(&path).ok() {
        Some(text) => toml::from_str(&text)
            .with_context(|| format!("{} is not valid TOML", path.display()))?,
        None => toml::Table::new(),
    };
    table.insert(
        "ui".into(),
        toml::Value::try_from(ui).context("writing the [ui] section")?,
    );
    let body = format!(
        "# Sources and outputs managed from the GodwinMix UI or API, and the\n\
         # surface defaults a preset chose. These lists take precedence over the\n\
         # ones in the config file. Delete this file to go back to it.\n\n{}",
        toml::to_string_pretty(&table).context("writing the runtime store")?
    );
    write_atomic(&path, body.as_bytes())?;
    Ok(path)
}

/// Write then rename, so a crash mid write cannot leave half a file.
fn write_atomic(path: &Path, body: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("making {}", parent.display()))?;
        }
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, body).with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))
}

impl Applied {
    /// What was done, for a terminal.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![format!("applied {}", self.preset)];
        for path in &self.wrote {
            out.push(format!("  wrote    {}", path.display()));
        }
        if let Some(backup) = &self.backup {
            out.push(format!(
                "  kept     {} (your file as it was)",
                backup.display()
            ));
        }
        if !self.todo.is_empty() {
            out.push(String::new());
            out.push("left for you".into());
            for (n, item) in self.todo.iter().enumerate() {
                out.push(format!("  {}. {item}", n + 1));
            }
        }
        if !self.steps.is_empty() {
            out.push(String::new());
            out.push("then".into());
            for (n, step) in self.steps.iter().enumerate() {
                out.push(format!("  {}. {step}", n + 1));
            }
        }
        out
    }
}

/// Every check the acceptance list asks for, in one place, so both the CLI and
/// the RPC method behave the same way.
pub fn apply_named(name: &str, options: &super::plan::Options) -> Result<(Plan, Applied)> {
    let preset = super::manifest::resolve(name)?;
    let plan = super::plan::build(&preset, options)?;
    let applied = run(&preset, &plan)?;
    Ok((plan, applied))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::plan::Options;

    fn work(tag: &str) -> PathBuf {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!(
            "gmx-apply-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn applying_church_to_a_fresh_directory_writes_a_config_that_loads() {
        let dir = work("fresh");
        let config = dir.join("godwinmix.toml");
        let (plan, applied) = apply_named("church", &Options::new(&config)).unwrap();

        assert!(config.exists());
        let text = std::fs::read_to_string(&config).unwrap();
        assert!(
            text.contains("# The church preset"),
            "the preset's comments came with it"
        );
        let loaded = Config::load(&config).unwrap();
        assert!(loaded.sources.iter().any(|s| s.id == "cam-wide"));
        assert!(loaded.outputs.iter().any(|o| o.id == "youtube"));

        assert!(plan.scenes_path.exists());
        let scenes =
            Collection::from_json(&std::fs::read_to_string(&plan.scenes_path).unwrap()).unwrap();
        assert_eq!(scenes.scenes.len(), plan.scenes.len());

        let store = Config::runtime_store_path(&config);
        let text = std::fs::read_to_string(&store).unwrap();
        assert!(text.contains("[ui]"), "{text}");
        assert_eq!(applied.ui.preset.as_deref(), Some("church"));
        assert_eq!(
            applied.ui.gallery.as_deref(),
            Some("icon"),
            "the church preset ships icon mode"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_config_that_was_already_there_keeps_its_values_and_its_old_self() {
        let dir = work("merge");
        let config = dir.join("godwinmix.toml");
        std::fs::write(
            &config,
            "[program]\nvideo_bitrate_kbps = 1500\n\n[[sources]]\nid = \"mine\"\nuri = \"rtmp://localhost/live/a\"\n",
        )
        .unwrap();

        let (_, applied) = apply_named("church", &Options::new(&config)).unwrap();
        let loaded = Config::load(&config).unwrap();
        assert_eq!(
            loaded.program.video_bitrate_kbps, 1500,
            "the operator's value won"
        );
        assert!(
            loaded.sources.iter().any(|s| s.id == "mine"),
            "their own source is still there"
        );
        assert!(
            loaded.sources.iter().any(|s| s.id == "cam-wide"),
            "and the preset's was appended"
        );
        assert!(applied.backup.is_some());
        let backup = std::fs::read_to_string(applied.backup.unwrap()).unwrap();
        assert!(backup.contains("1500"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn applying_the_same_preset_twice_does_not_double_anything() {
        let dir = work("twice");
        let config = dir.join("godwinmix.toml");
        apply_named("classroom", &Options::new(&config)).unwrap();
        let first = Config::load(&config).unwrap();
        let (plan, _) = apply_named("classroom", &Options::new(&config)).unwrap();
        let second = Config::load(&config).unwrap();
        assert_eq!(first.sources.len(), second.sources.len());
        assert_eq!(first.outputs.len(), second.outputs.len());
        let scenes =
            Collection::from_json(&std::fs::read_to_string(&plan.scenes_path).unwrap()).unwrap();
        assert_eq!(scenes.scenes.len(), plan.scenes.len());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn keep_sources_leaves_the_operators_list_alone() {
        let dir = work("keep");
        let config = dir.join("godwinmix.toml");
        std::fs::write(
            &config,
            "[[sources]]\nid = \"mine\"\nuri = \"rtmp://localhost/live/a\"\n",
        )
        .unwrap();
        let mut options = Options::new(&config);
        options.keep_sources = true;
        apply_named("church", &options).unwrap();
        let loaded = Config::load(&config).unwrap();
        assert_eq!(loaded.sources.len(), 1, "no source was added");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn every_official_preset_applies_to_a_fresh_directory_and_starts_up() {
        for name in crate::preset::manifest::NAMES {
            let dir = work(name);
            let config = dir.join("godwinmix.toml");
            let (_, applied) = apply_named(name, &Options::new(&config))
                .unwrap_or_else(|e| panic!("{name}: {e:#}"));
            Config::load(&config).unwrap_or_else(|e| panic!("{name}: {e:#}"));
            assert_eq!(applied.wrote.len(), 3, "{name} wrote {:?}", applied.wrote);
            std::fs::remove_dir_all(&dir).ok();
        }
    }
}
