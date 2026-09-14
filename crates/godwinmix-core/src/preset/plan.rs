//! What applying a preset would do, worked out before anything is written.
//!
//! The plan is the whole of `--dry-run` and the first half of a real apply. It
//! is also what the welcome panel shows a volunteer: the plugins that are
//! missing, the keys that will be set, the sources and outputs that will
//! appear, and the three steps left for the person.
//!
//! A plan fails to build only on something genuinely wrong with the preset: a
//! file it names that is not there, a config this build cannot load, a scene
//! that does not validate, a layout naming a panel that does not exist. A
//! plugin that is not installed is reported, never fatal, because a preset is
//! a target the plugins are written towards (06 section 5).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use super::manifest::{Preset, PresetBlock, PluginSpec, BUILT_IN_THEMES, GALLERY_MODES, PANELS, SLOTS};
use crate::config::Config;
use crate::scene::document::{Collection, Scene};
use crate::scene::{layout, validate};

/// A plugin the preset names, and whether this build already has it.
#[derive(Debug, Clone, Serialize)]
pub struct PluginNeed {
    /// As written in the manifest, for example `ndi@^1`.
    pub spec: String,
    pub name: String,
    pub range: String,
    pub installed: bool,
    /// The provides this build carries under that plugin name.
    pub provides: Vec<String>,
}

/// What happens to one configuration key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    /// The operator's config has no such key. The preset fills it.
    Set,
    /// The operator has their own value and it wins.
    Keep,
    /// `--force`: the preset's value replaces the operator's.
    Override,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConfigChange {
    /// A dotted path, for example `program.video_bitrate_kbps`.
    pub key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    pub to: String,
    pub action: Action,
}

/// A source or an output the preset brings.
#[derive(Debug, Clone, Serialize)]
pub struct Addition {
    pub id: String,
    pub type_id: String,
    pub uri: String,
    /// True when the operator already has one with this id, so it is left alone.
    pub already_there: bool,
    /// The plugin that has to be installed before this one runs, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub needs_plugin: Option<String>,
}

/// Everything `gmx preset apply` would do.
#[derive(Debug, Clone, Serialize)]
pub struct Plan {
    pub name: String,
    pub origin: String,
    pub description: String,
    pub surface: String,
    pub theme: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_css: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gallery: Option<String>,
    pub steps: Vec<String>,
    pub plugins: Vec<PluginNeed>,
    pub config: Vec<ConfigChange>,
    pub sources: Vec<Addition>,
    pub outputs: Vec<Addition>,
    pub scenes: Vec<String>,
    pub layout: BTreeMap<String, Vec<String>>,
    /// What is left for the person afterwards, in the order they do it.
    pub todo: Vec<String>,
    pub config_path: PathBuf,
    pub scenes_path: PathBuf,
    pub force: bool,
    pub keep_sources: bool,
}

/// How the plan is built.
#[derive(Debug, Clone)]
pub struct Options {
    /// The operator's config file. It need not exist yet.
    pub config_path: PathBuf,
    /// Preset values replace the operator's rather than filling gaps.
    pub force: bool,
    /// Leave the operator's sources and outputs alone.
    pub keep_sources: bool,
}

impl Options {
    pub fn new(config_path: impl Into<PathBuf>) -> Self {
        Self { config_path: config_path.into(), force: false, keep_sources: false }
    }
}

/// Work out what applying `preset` to this machine would do.
pub fn build(preset: &Preset, options: &Options) -> Result<Plan> {
    let block = preset.block()?.clone();
    check_files(preset, &block)?;
    let preset_config = preset.read(&block.config)?;
    let parsed = Config::from_toml(&preset_config, &format!("{}'s config", preset.name))
        .with_context(|| format!("the preset {} does not load on this build", preset.name))?;
    let preset_table: toml::Table = toml::from_str(&preset_config)
        .with_context(|| format!("parsing {}'s config", preset.name))?;
    let current = read_current(&options.config_path)?;

    let layout = read_layout(preset, &block)?;
    check_theme(preset, &block)?;
    check_gallery(&block)?;
    let scenes = resolve_scenes(preset, &block, &parsed)?;

    let plugins = needed_plugins(&block);
    let sources = additions(
        preset_table.get("sources"),
        current.get("sources"),
        &plugins,
        options.keep_sources,
    );
    let outputs = additions(
        preset_table.get("outputs"),
        current.get("outputs"),
        &plugins,
        options.keep_sources,
    );
    let config = diff(&preset_table, &current, options.force);

    let mut plan = Plan {
        name: preset.name.clone(),
        origin: preset.origin(),
        description: preset.description().to_string(),
        surface: block.surface.clone(),
        theme: block.theme.clone(),
        theme_css: block.theme_css.clone(),
        gallery: block.gallery.clone(),
        steps: block.steps.clone(),
        plugins,
        config,
        sources,
        outputs,
        scenes: scenes.iter().map(|s| s.name.clone()).collect(),
        layout,
        todo: Vec::new(),
        scenes_path: scenes_path(&options.config_path),
        config_path: options.config_path.clone(),
        force: options.force,
        keep_sources: options.keep_sources,
    };
    plan.todo = todo(&plan, &preset_config);
    Ok(plan)
}

/// Where the scene collection goes: `godwinmix.toml` gives `godwinmix.scenes.json`.
pub fn scenes_path(config: &Path) -> PathBuf {
    let mut name = config.file_stem().unwrap_or_default().to_os_string();
    name.push(".scenes.json");
    config.with_file_name(name)
}

fn check_files(preset: &Preset, block: &PresetBlock) -> Result<()> {
    for path in [&block.config, &block.layout, &block.scenes] {
        anyhow::ensure!(
            preset.has(path),
            "the preset {} names {path:?} and there is no such file in {}",
            preset.name,
            preset.origin()
        );
    }
    if let Some(css) = &block.theme_css {
        anyhow::ensure!(
            preset.has(css),
            "the preset {} names the stylesheet {css:?} and there is no such file in {}",
            preset.name,
            preset.origin()
        );
    }
    Ok(())
}

fn check_theme(preset: &Preset, block: &PresetBlock) -> Result<()> {
    if BUILT_IN_THEMES.contains(&block.theme.as_str()) || block.theme_css.is_some() {
        return Ok(());
    }
    anyhow::bail!(
        "the preset {} names the theme {:?}, which is not one of {} and is not a \
         theme.css inside the preset. Ship one as `theme_css = \"theme.css\"` or name a \
         built in theme",
        preset.name,
        block.theme,
        BUILT_IN_THEMES.join(", ")
    )
}

fn check_gallery(block: &PresetBlock) -> Result<()> {
    let Some(mode) = &block.gallery else { return Ok(()) };
    anyhow::ensure!(
        GALLERY_MODES.contains(&mode.as_str()),
        "gallery = {mode:?} is not a tile mode. It is one of {}",
        GALLERY_MODES.join(", ")
    );
    Ok(())
}

fn read_layout(preset: &Preset, block: &PresetBlock) -> Result<BTreeMap<String, Vec<String>>> {
    let text = preset.read(&block.layout)?;
    let layout: BTreeMap<String, Vec<String>> = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a slot to panel list map", block.layout))?;
    for (slot, panels) in &layout {
        anyhow::ensure!(
            SLOTS.contains(&slot.as_str()),
            "{} puts panels in {slot:?}, which is not a UI slot. They are {}",
            block.layout,
            SLOTS.join(", ")
        );
        for panel in panels {
            anyhow::ensure!(
                PANELS.contains(&panel.as_str()) || panel.contains('/'),
                "{} names the panel {panel:?}, which no first party panel is called. \
                 A plugin's panel is written as `<plugin>/<panel>`",
                block.layout
            );
        }
    }
    Ok(layout)
}

/// Every scene the preset ships, with its layout bindings filled in from the
/// preset's own sources. A layout with a binding left over is an error here
/// rather than a broken scene on a volunteer's Sunday.
pub fn resolve_scenes(
    preset: &Preset,
    block: &PresetBlock,
    config: &Config,
) -> Result<Vec<Scene>> {
    let mut out = Vec::new();
    for (file, text) in preset.json_files(&block.scenes)? {
        let doc = Collection::from_json(&text)
            .with_context(|| format!("{}/{file} is not a scene document", preset.name))?;
        let findings = validate::collection(&doc);
        anyhow::ensure!(
            !validate::has_errors(&findings),
            "{}/{file} does not validate: {findings:#?}",
            preset.name
        );
        let values = bindings(config, &doc);
        let scene = layout::apply(&doc, &values, doc.canvas)
            .with_context(|| format!("{}/{file} does not resolve", preset.name))?;
        let text = serde_json::to_string(&scene).unwrap_or_default();
        anyhow::ensure!(
            !text.contains("{{"),
            "{}/{file} has a binding left unfilled after every source was offered",
            preset.name
        );
        out.push(scene);
    }
    anyhow::ensure!(!out.is_empty(), "{} ships no scenes", preset.name);
    Ok(out)
}

/// Fill a layout's slots from the preset's own sources, in order, repeating
/// when the layout wants more slots than the preset has sources.
fn bindings(config: &Config, doc: &Collection) -> layout::Values {
    let mut values = layout::Values::new();
    if config.sources.is_empty() {
        return values;
    }
    let mut ids = config.sources.iter().map(|s| s.id.clone()).cycle();
    let properties = doc.params.get("properties").and_then(serde_json::Value::as_object);
    for (key, schema) in properties.into_iter().flatten() {
        match schema.get("x-gmx-kind").and_then(serde_json::Value::as_str) {
            Some("source") => {
                if let Some(id) = ids.next() {
                    values.insert(key.clone(), serde_json::Value::from(id));
                }
            }
            Some("graphic") => {
                values.insert(key.clone(), serde_json::Value::from("lowerthird/graphic"));
            }
            _ => {}
        }
    }
    values
}

/// The operator's config file as a raw table, or an empty one when there is
/// none yet. Raw, because a key the operator did not write is a key the preset
/// may fill, and a deserialised `Config` cannot tell the two apart.
fn read_current(path: &Path) -> Result<toml::Table> {
    let real = crate::config::path_in_force(path);
    if !real.exists() {
        return Ok(toml::Table::new());
    }
    let text = std::fs::read_to_string(&real)
        .with_context(|| format!("reading {}", real.display()))?;
    toml::from_str(&text).with_context(|| {
        format!("{} is not valid TOML, so there is nothing to merge into", real.display())
    })
}

/// Which plugins the preset needs, and whether this build already provides them.
fn needed_plugins(block: &PresetBlock) -> Vec<PluginNeed> {
    let built_in: Vec<String> = crate::plugin::source::available()
        .into_iter()
        .chain(crate::plugin::output::available())
        .chain(crate::plugin::filter::available())
        .collect();
    block
        .plugins
        .iter()
        .map(|spec| {
            let parsed = PluginSpec::parse(spec);
            let prefix = format!("{}/", parsed.name);
            let provides: Vec<String> =
                built_in.iter().filter(|p| p.starts_with(&prefix)).cloned().collect();
            PluginNeed {
                spec: spec.clone(),
                installed: !provides.is_empty(),
                name: parsed.name,
                range: parsed.range,
                provides,
            }
        })
        .collect()
}

/// The sources or outputs a preset adds, matched against the ones already there.
fn additions(
    from_preset: Option<&toml::Value>,
    from_operator: Option<&toml::Value>,
    plugins: &[PluginNeed],
    keep: bool,
) -> Vec<Addition> {
    let existing: Vec<String> = from_operator
        .and_then(toml::Value::as_array)
        .map(|a| a.iter().filter_map(|v| field(v, "id")).collect())
        .unwrap_or_default();
    let Some(list) = from_preset.and_then(toml::Value::as_array) else { return Vec::new() };
    list.iter()
        .filter_map(|entry| {
            let id = field(entry, "id")?;
            let type_id = field(entry, "type").unwrap_or_default();
            let plugin = type_id.split('/').next().unwrap_or("");
            let needs_plugin = plugins
                .iter()
                .find(|p| p.name == plugin && !p.installed)
                .map(|p| p.name.clone());
            Some(Addition {
                already_there: keep || existing.contains(&id),
                id,
                uri: field(entry, "uri").unwrap_or_default(),
                type_id,
                needs_plugin,
            })
        })
        .collect()
}

fn field(value: &toml::Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_string)
}

/// Every scalar key the preset would set, against what the operator has.
///
/// `sources` and `outputs` are arrays of tables and are handled by `additions`,
/// so they are skipped here; `[codecs]` is a catalogue, not a setting, and a
/// preset that carries one replaces it whole.
fn diff(preset: &toml::Table, current: &toml::Table, force: bool) -> Vec<ConfigChange> {
    let mut out = Vec::new();
    walk(preset, current, "", force, &mut out);
    out.sort_by(|a, b| a.key.cmp(&b.key));
    out
}

fn walk(
    preset: &toml::Table,
    current: &toml::Table,
    prefix: &str,
    force: bool,
    out: &mut Vec<ConfigChange>,
) {
    for (key, value) in preset {
        if prefix.is_empty() && matches!(key.as_str(), "sources" | "outputs") {
            continue;
        }
        let path = if prefix.is_empty() { key.clone() } else { format!("{prefix}.{key}") };
        let here = current.get(key);
        match (value, here) {
            (toml::Value::Table(sub), Some(toml::Value::Table(mine))) => {
                walk(sub, mine, &path, force, out)
            }
            (toml::Value::Table(sub), _) => walk(sub, &toml::Table::new(), &path, force, out),
            (_, None) => out.push(ConfigChange {
                key: path,
                from: None,
                to: short(value),
                action: Action::Set,
            }),
            (_, Some(mine)) if mine == value => {}
            (_, Some(mine)) => out.push(ConfigChange {
                key: path,
                from: Some(short(mine)),
                to: short(value),
                action: if force { Action::Override } else { Action::Keep },
            }),
        }
    }
}

fn short(value: &toml::Value) -> String {
    match value {
        toml::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Anything the person still has to do, in the order they do it.
fn todo(plan: &Plan, config_text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for plugin in plan.plugins.iter().filter(|p| !p.installed) {
        out.push(format!(
            "install the {} plugin: `gmx plugin add {}` (the sources that need it stay \
             listed and do not start until it is there)",
            plugin.name, plugin.name
        ));
    }
    // Only what is actually in force. A placeholder behind a `#` is the
    // preset explaining an option, not a value anybody has to replace.
    let live: String = config_text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for marker in ["YOUR-STREAM-KEY", "change-me", "CHANGE-ME", "YOUR-KEY"] {
        if live.contains(marker) {
            out.push(format!(
                "replace {marker} in {} with the real value",
                plan.config_path.display()
            ));
        }
    }
    if plan.config.iter().any(|c| c.action == Action::Keep) {
        let kept = plan.config.iter().filter(|c| c.action == Action::Keep).count();
        out.push(format!(
            "{kept} key(s) you already set were left alone. `--force` takes the preset's \
             values instead"
        ));
    }
    out
}

impl Plan {
    /// The plugins that are not here yet.
    pub fn missing(&self) -> Vec<&PluginNeed> {
        self.plugins.iter().filter(|p| !p.installed).collect()
    }

    /// The plan as lines for a terminal. One screen for a working preset.
    pub fn report(&self) -> Vec<String> {
        let mut out = vec![
            format!("preset {} ({})", self.name, self.origin),
            format!("  {}", self.description),
            String::new(),
        ];
        out.push("plugins".into());
        if self.plugins.is_empty() {
            out.push("  none: everything it uses is built in".into());
        }
        for p in &self.plugins {
            let mark = if p.installed { "have" } else { "MISSING" };
            let how = if p.installed {
                p.provides.join(", ")
            } else {
                format!("`gmx plugin add {}`", p.name)
            };
            out.push(format!("  {mark:<8} {:<16} {how}", p.spec));
        }
        out.push(String::new());
        out.push(format!("config {}", self.config_path.display()));
        for c in &self.config {
            let line = match c.action {
                Action::Set => format!("  set      {} = {}", c.key, c.to),
                Action::Keep => format!(
                    "  keep     {} = {} (the preset wanted {})",
                    c.key,
                    c.from.clone().unwrap_or_default(),
                    c.to
                ),
                Action::Override => format!(
                    "  replace  {} = {} (was {})",
                    c.key,
                    c.to,
                    c.from.clone().unwrap_or_default()
                ),
            };
            out.push(line);
        }
        out.extend(self.additions_report("sources", &self.sources));
        out.extend(self.additions_report("outputs", &self.outputs));
        out.push(String::new());
        out.push(format!("scenes {}", self.scenes_path.display()));
        for s in &self.scenes {
            out.push(format!("  add      {s}"));
        }
        out.push(String::new());
        out.push("surface".into());
        out.push(format!("  layout   {}", slots(&self.layout)));
        out.push(format!("  theme    {}", self.theme));
        out.push(format!(
            "  gallery  {}",
            self.gallery.clone().unwrap_or_else(|| "whatever this machine can afford".into())
        ));
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

    fn additions_report(&self, what: &str, items: &[Addition]) -> Vec<String> {
        if items.is_empty() {
            return Vec::new();
        }
        let mut out = vec![String::new(), what.to_string()];
        for a in items {
            let note = match (&a.already_there, &a.needs_plugin) {
                (true, _) => "kept: you already have one with this id".to_string(),
                (false, Some(p)) => format!("waits for the {p} plugin"),
                (false, None) => a.uri.clone(),
            };
            let verb = if a.already_there { "keep" } else { "add" };
            out.push(format!("  {verb:<8} {:<14} {:<16} {note}", a.id, a.type_id));
        }
        out
    }
}

fn slots(layout: &BTreeMap<String, Vec<String>>) -> String {
    layout
        .iter()
        .map(|(slot, panels)| format!("{slot}: {}", panels.join(" ")))
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::manifest;

    fn plan_for(name: &str, config: &Path) -> Plan {
        let _ = gstreamer::init();
        let preset = manifest::resolve(name).unwrap();
        build(&preset, &Options::new(config)).unwrap_or_else(|e| panic!("{name}: {e:#}"))
    }

    #[test]
    fn every_official_preset_plans_against_a_machine_with_nothing_on_it() {
        for name in manifest::NAMES {
            let plan = plan_for(name, Path::new("/nowhere/godwinmix.toml"));
            assert!(!plan.scenes.is_empty(), "{name} adds no scenes");
            assert!(!plan.layout.is_empty(), "{name} sets no layout");
            assert!(!plan.config.is_empty(), "{name} sets no config");
            assert!(
                plan.config.iter().all(|c| c.action == Action::Set),
                "{name}: nothing is there to keep on a fresh machine"
            );
        }
    }

    #[test]
    fn the_church_preset_names_the_camera_plugin_as_missing_and_nothing_else() {
        let plan = plan_for("church", Path::new("/nowhere/godwinmix.toml"));
        let missing: Vec<&str> = plan.missing().iter().map(|p| p.name.as_str()).collect();
        assert!(missing.contains(&"camera"), "the camera plugin is what is missing: {missing:?}");
        for name in ["browser", "rtmp", "file"] {
            assert!(!missing.contains(&name), "{name} is built in, so it is not missing");
        }
        assert!(plan.todo.iter().any(|t| t.contains("camera")), "{:?}", plan.todo);
    }

    #[test]
    fn a_key_the_operator_already_set_is_kept_and_force_takes_it() {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("godwinmix.toml");
        std::fs::write(&path, "[program]\nvideo_bitrate_kbps = 1234\n").unwrap();

        let preset = manifest::resolve("church").unwrap();
        let plan = build(&preset, &Options::new(&path)).unwrap();
        let change = plan
            .config
            .iter()
            .find(|c| c.key == "program.video_bitrate_kbps")
            .expect("the key is in the diff");
        assert_eq!(change.action, Action::Keep);
        assert_eq!(change.from.as_deref(), Some("1234"));

        let mut options = Options::new(&path);
        options.force = true;
        let forced = build(&preset, &options).unwrap();
        let change =
            forced.config.iter().find(|c| c.key == "program.video_bitrate_kbps").unwrap();
        assert_eq!(change.action, Action::Override);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_preset_naming_a_theme_nobody_has_does_not_plan() {
        let _ = gstreamer::init();
        let mut preset = manifest::resolve("church").unwrap();
        let block = preset.manifest.provides[0].preset.as_mut().unwrap();
        block.theme = "chartreuse".into();
        block.theme_css = None;
        let e = build(&preset, &Options::new(Path::new("/nowhere/godwinmix.toml")))
            .unwrap_err()
            .to_string();
        assert!(e.contains("chartreuse"), "{e}");
        assert!(e.contains("theme_css"), "{e}");
    }

    #[test]
    fn the_scene_file_sits_beside_the_config_file() {
        assert_eq!(
            scenes_path(Path::new("/srv/show/godwinmix.toml")),
            Path::new("/srv/show/godwinmix.scenes.json")
        );
    }

    #[test]
    fn the_report_is_one_screen_and_names_the_missing_plugin() {
        let plan = plan_for("church", Path::new("/nowhere/godwinmix.toml"));
        let text = plan.report().join("\n");
        assert!(text.contains("MISSING"), "{text}");
        assert!(text.contains("gmx plugin add camera"), "{text}");
        assert!(plan.report().len() < 80, "the plan is {} lines", plan.report().len());
    }
}
