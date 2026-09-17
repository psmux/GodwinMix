//! `gmx preset save`: turn what is working here into a preset somebody else
//! can apply.
//!
//! This is how the second preset gets written, and the third. A volunteer who
//! got their church on air runs one command and has a directory they can hand
//! to the next church, or tag and put on the index. Everything it writes is
//! the shape `presets/README.md` documents, so the result is a preset the same
//! loader reads back.
//!
//! Secrets never leave: the control token, the `[[tokens]]` table and the tail
//! of every output URL are replaced with the placeholders the official presets
//! use, so a saved preset is safe to publish by default.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::config::{Config, UiDefaults};
use crate::scene::document::Collection;
use crate::scene::Id;

/// What `save` wrote.
#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub name: String,
    pub dir: PathBuf,
    pub wrote: Vec<PathBuf>,
    /// Placeholders the author has to check before publishing.
    pub redacted: Vec<String>,
    /// Plugins the manifest now names, worked out from the kinds in use.
    pub plugins: Vec<String>,
}

/// Where a saved preset goes when nothing says otherwise.
pub fn default_dir(name: &str) -> PathBuf {
    match super::manifest::home_dir() {
        Some(home) => home.join(".godwinmix").join("presets").join(name),
        None => PathBuf::from("presets").join(name),
    }
}

/// Write a preset directory from a working configuration.
pub fn run(name: &str, config_path: &Path, out: &Path) -> Result<Saved> {
    anyhow::ensure!(
        !name.is_empty()
            && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
        "a preset name is a slug: lower case letters, digits and hyphens. {name:?} is not one"
    );
    let real = crate::config::path_in_force(config_path);
    let config = Config::load(&real).with_context(|| {
        format!("{} does not load, so there is nothing worth saving", real.display())
    })?;
    let original = std::fs::read_to_string(&real)
        .with_context(|| format!("reading {}", real.display()))?;

    std::fs::create_dir_all(out.join("config"))
        .with_context(|| format!("making {}", out.join("config").display()))?;
    std::fs::create_dir_all(out.join("scenes"))
        .with_context(|| format!("making {}", out.join("scenes").display()))?;

    let (body, redacted) = redact(&original, &config);
    let mut wrote = Vec::new();
    write(out.join("config/godwinmix.toml"), &body, &mut wrote)?;

    let mut ui = config.ui.clone();
    let theme_css = carry_theme(&mut ui, out, &mut wrote)?;
    let layout = if ui.layout.is_empty() { default_layout() } else { ui.layout.clone() };
    write(
        out.join("config/layout.json"),
        &serde_json::to_string_pretty(&layout).context("writing the layout")?,
        &mut wrote,
    )?;

    let scenes = copy_scenes(&super::plan::scenes_path(&real), out, &mut wrote)?;
    let plugins = plugins_in_use(&config);
    write(
        out.join("gmx-plugin.toml"),
        &manifest_text(name, &plugins, &ui, theme_css, &config),
        &mut wrote,
    )?;
    write(out.join("README.md"), &readme(name, &config, scenes), &mut wrote)?;

    Ok(Saved { name: name.to_string(), dir: out.to_path_buf(), wrote, redacted, plugins })
}

fn write(path: PathBuf, body: &str, wrote: &mut Vec<PathBuf>) -> Result<()> {
    std::fs::write(&path, body).with_context(|| format!("writing {}", path.display()))?;
    wrote.push(path);
    Ok(())
}

/// Take the secrets out, and say which ones were taken.
fn redact(original: &str, config: &Config) -> (String, Vec<String>) {
    let mut notes = Vec::new();
    let mut out = String::with_capacity(original.len());
    let mut in_tokens = false;
    for line in original.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("[[tokens]]") {
            in_tokens = true;
            notes.push("the [[tokens]] table was dropped".into());
            continue;
        }
        if in_tokens {
            if trimmed.starts_with('[') {
                in_tokens = false;
            } else {
                continue;
            }
        }
        if let Some(rest) = trimmed.strip_prefix("token") {
            if rest.trim_start().starts_with('=') {
                out.push_str("# token = \"change-me\"\n");
                notes.push("the control token was replaced with change-me".into());
                continue;
            }
        }
        match stream_key(trimmed) {
            Some(replacement) => {
                notes.push(format!("a stream key in {} was replaced", trimmed.trim()));
                out.push_str(&replacement);
                out.push('\n');
            }
            None => {
                out.push_str(line);
                out.push('\n');
            }
        }
    }
    let header = format!(
        "# Saved by `gmx preset save` from a working {}x{} configuration.\n\
         # Check every value below before you hand this to anybody: the stream keys and\n\
         # the control token were replaced, and nothing else was.\n\n",
        config.canvas.width, config.canvas.height
    );
    (header + &out, notes)
}

/// `uri = "rtmp://host/app/secret"` becomes the same with a placeholder tail.
fn stream_key(line: &str) -> Option<String> {
    let rest = line.strip_prefix("uri")?.trim_start().strip_prefix('=')?.trim();
    let value = rest.trim_matches('"');
    if !(value.starts_with("rtmp://")
        || value.starts_with("rtmps://")
        || value.starts_with("srt://"))
    {
        return None;
    }
    let (head, tail) = value.rsplit_once('/')?;
    if tail.is_empty() || tail.len() < 6 {
        return None;
    }
    Some(format!("uri = \"{head}/YOUR-STREAM-KEY\""))
}

/// Bring the theme with the preset.
///
/// A theme this build does not carry came from a preset, and a preset that
/// names a theme nobody has is one the loader refuses. Copy the stylesheet in
/// beside the manifest, or fall back to the default theme and say nothing more
/// about it: a saved preset must be one that applies.
fn carry_theme(ui: &mut UiDefaults, out: &Path, wrote: &mut Vec<PathBuf>) -> Result<bool> {
    let Some(theme) = ui.theme.clone() else { return Ok(false) };
    if super::manifest::BUILT_IN_THEMES.contains(&theme.as_str()) {
        return Ok(false);
    }
    let css = ui
        .preset
        .as_deref()
        .and_then(|from| super::manifest::resolve(from).ok())
        .and_then(|preset| {
            let name = preset.block().ok()?.theme_css.clone()?;
            preset.read(&name).ok()
        });
    match css {
        Some(body) => {
            write(out.join("theme.css"), &body, wrote)?;
            Ok(true)
        }
        None => {
            ui.theme = Some("dark".into());
            Ok(false)
        }
    }
}

fn default_layout() -> BTreeMap<String, Vec<String>> {
    BTreeMap::from([
        ("header".to_string(), vec!["header".to_string()]),
        ("main".to_string(), vec!["scenes".to_string(), "multiview".to_string()]),
        ("sidebar".to_string(), vec!["sources".to_string()]),
        ("footer".to_string(), vec!["alerts".to_string(), "outputs".to_string()]),
    ])
}

/// Copy the collection's scenes out, one file each, with fresh ids.
///
/// Ids are never reused across documents (`presets/README.md`), because two
/// presets applied to the same core would collide. Every id in the document is
/// mapped to a new one, references included.
fn copy_scenes(collection: &Path, out: &Path, wrote: &mut Vec<PathBuf>) -> Result<usize> {
    let Ok(text) = std::fs::read_to_string(collection) else { return Ok(0) };
    let doc = Collection::from_json(&text)
        .with_context(|| format!("{} is not a scene collection", collection.display()))?;
    let mut count = 0;
    for scene in &doc.scenes {
        let one = Collection {
            schema_version: doc.schema_version,
            id: Id::new(),
            name: scene.name.clone(),
            canvas: doc.canvas,
            params: crate::scene::document::empty_params(),
            scenes: vec![scene.clone()],
            transitions: Vec::new(),
            assets: doc.assets.clone(),
            sources: doc.sources.clone(),
        };
        let mut value = serde_json::to_value(&one).context("writing a scene")?;
        refresh_ids(&mut value, &mut BTreeMap::new());
        let body = serde_json::to_string_pretty(&value).context("writing a scene")?;
        write(out.join("scenes").join(format!("{}.json", slug(&scene.name))), &body, wrote)?;
        count += 1;
    }
    Ok(count)
}

/// Give every id in a document a fresh value, consistently.
fn refresh_ids(value: &mut serde_json::Value, map: &mut BTreeMap<String, String>) {
    match value {
        serde_json::Value::Object(obj) => {
            for (key, child) in obj.iter_mut() {
                if key == "id" {
                    if let Some(old) = child.as_str() {
                        let fresh = map
                            .entry(old.to_string())
                            .or_insert_with(|| Id::new().to_string())
                            .clone();
                        *child = serde_json::Value::String(fresh);
                        continue;
                    }
                }
                refresh_ids(child, map);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                refresh_ids(item, map);
            }
        }
        _ => {}
    }
}

fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "scene".into() } else { trimmed }
}

/// The plugins a config needs, from the kinds its sources and outputs name.
fn plugins_in_use(config: &Config) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    let kinds = config
        .sources
        .iter()
        .filter_map(|s| s.type_id.clone())
        .chain(config.outputs.iter().filter_map(|o| o.type_id.clone()));
    for kind in kinds {
        let Some(plugin) = kind.split('/').next() else { continue };
        if plugin.is_empty() || names.iter().any(|n| n == plugin) {
            continue;
        }
        names.push(plugin.to_string());
    }
    names.sort();
    names.into_iter().map(|n| format!("{n}@^1")).collect()
}

/// One `[[provides.preset.next]]` table per destination that still wants a
/// key, and a take at the end. Typed, because the welcome panel draws a
/// control for each one and cannot draw anything from a sentence.
fn next_tables(config: &Config) -> String {
    let mut out = String::new();
    for output in &config.outputs {
        if godwinmix_protocol::types::uri_has_key(&output.uri) {
            continue;
        }
        out.push_str(&format!(
            "\n[[provides.preset.next]]\ndo = \"stream_key\"\noutput = {:?}\n\
             text = \"Say here where this key is found.\"\n",
            output.id
        ));
    }
    match config.sources.first() {
        Some(source) => out.push_str(&format!(
            "\n[[provides.preset.next]]\ndo = \"take\"\nsource = {:?}\n\
             text = \"Press it to put a picture on air.\"\n",
            source.id
        )),
        None => out.push_str(
            "\n[[provides.preset.next]]\ndo = \"add_source\"\nkind = \"rtmp/source\"\n\
             text = \"Say here what to point at this mixer.\"\n",
        ),
    }
    out.push_str(
        "\n[[provides.preset.next]]\ndo = \"note\"\n\
         text = \"Replace this with the one other thing somebody has to know.\"\n",
    );
    out
}

fn manifest_text(
    name: &str,
    plugins: &[String],
    ui: &UiDefaults,
    theme_css: bool,
    config: &Config,
) -> String {
    let list = plugins.iter().map(|p| format!("{p:?}")).collect::<Vec<_>>().join(", ");
    let theme = ui.theme.clone().unwrap_or_else(|| "dark".into());
    let gallery = match &ui.gallery {
        Some(mode) => format!("gallery = {mode:?}\n"),
        None => String::new(),
    };
    let css = if theme_css { "theme_css = \"theme.css\"\n" } else { "" };
    let next = next_tables(config);
    format!(
        "[plugin]\n\
         name = {name:?}\n\
         version = \"0.1.0\"\n\
         api = 1\n\
         description = \"Saved from a working GodwinMix. Say here what this setup is for, in \
         a sentence somebody browsing the index would understand.\"\n\
         license = \"Apache-2.0\"\n\
         \n\
         [[provides]]\n\
         kind = \"preset\"\n\
         id = {name:?}\n\
         \n\
         [provides.preset]\n\
         plugins = [{list}]\n\
         config = \"config/godwinmix.toml\"\n\
         layout = \"config/layout.json\"\n\
         surface = \"web\"\n\
         theme = {theme:?}\n\
         scenes = \"scenes\"\n\
         {css}\
         {gallery}\
         {next}"
    )
}

fn readme(name: &str, config: &Config, scenes: usize) -> String {
    format!(
        "# The {name} preset\n\n\
         Saved from a working GodwinMix with `gmx preset save {name}`. Rewrite every\n\
         section below for the person who will use it, in under 300 words.\n\n\
         ## What it gives you\n\n\
         {} source(s), {} destination(s) and {scenes} scene(s), on a {}x{} canvas at {} fps.\n\n\
         ## What you need\n\n\
         Say what has to exist before this works: the cameras, the addresses, the keys.\n\n\
         ## Three steps\n\n\
         1. Pick this setup on the welcome page, or apply it with `gmx preset apply {name}`.\n\
         2. Finish the checklist the page puts up: the stream keys, and any plugin it offers to install.\n\
         3. Press a tile to put a picture on air.\n\n\
         ## When it does not work\n\n\
         Say what usually goes wrong here and what fixes it. One paragraph each.\n",
        config.sources.len(),
        config.outputs.len(),
        config.canvas.width,
        config.canvas.height,
        config.canvas.fps,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset::plan::Options;

    fn work(tag: &str) -> PathBuf {
        let _ = gstreamer::init();
        let dir = std::env::temp_dir().join(format!("gmx-save-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_saved_preset_is_one_the_loader_reads_back_and_can_apply() {
        let dir = work("roundtrip");
        let config = dir.join("godwinmix.toml");
        crate::preset::apply::apply_named("church", &Options::new(&config)).unwrap();

        let out = dir.join("mine");
        let saved = run("my-church", &config, &out).unwrap();
        assert_eq!(saved.wrote.len(), 5 + 4, "config, layout, theme, manifest, README and four scenes");

        let preset = crate::preset::manifest::load(&out).unwrap();
        assert_eq!(preset.name, "my-church");
        let plan = crate::preset::plan::build(&preset, &Options::new(dir.join("second.toml")))
            .unwrap_or_else(|e| panic!("{e:#}"));
        assert!(!plan.scenes.is_empty());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_saved_preset_carries_no_stream_key_and_no_token() {
        let dir = work("secrets");
        let config = dir.join("godwinmix.toml");
        std::fs::write(
            &config,
            "[control]\nbind = \"127.0.0.1:8080\"\ntoken = \"hunter2-the-real-one\"\n\n\
             [[outputs]]\nid = \"yt\"\ntype = \"rtmp/output\"\nuri = \"rtmp://a.rtmp.youtube.com/live2/abcd-efgh-ijkl\"\n\n\
             [[sources]]\nid = \"cam\"\ntype = \"test/source\"\nuri = \"test://smpte\"\n",
        )
        .unwrap();
        let out = dir.join("mine");
        let saved = run("mine", &config, &out).unwrap();
        let text = std::fs::read_to_string(out.join("config/godwinmix.toml")).unwrap();
        assert!(!text.contains("hunter2"), "{text}");
        assert!(!text.contains("abcd-efgh-ijkl"), "{text}");
        assert!(text.contains("YOUR-STREAM-KEY"), "{text}");
        assert_eq!(saved.plugins, vec!["rtmp@^1", "test@^1"]);
        assert!(saved.redacted.len() >= 2, "{:?}", saved.redacted);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn two_saves_of_the_same_scenes_do_not_share_an_id() {
        let dir = work("ids");
        let config = dir.join("godwinmix.toml");
        crate::preset::apply::apply_named("default", &Options::new(&config)).unwrap();
        run("one", &config, &dir.join("one")).unwrap();
        run("two", &config, &dir.join("two")).unwrap();
        let first = std::fs::read_to_string(
            std::fs::read_dir(dir.join("one/scenes")).unwrap().next().unwrap().unwrap().path(),
        )
        .unwrap();
        let second_dir = std::fs::read_dir(dir.join("two/scenes")).unwrap();
        for entry in second_dir {
            let body = std::fs::read_to_string(entry.unwrap().path()).unwrap();
            for line in body.lines().filter(|l| l.contains("\"id\"")) {
                assert!(!first.contains(line.trim()), "an id was reused: {line}");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_name_that_is_not_a_slug_is_refused_with_the_rule() {
        let e = run("My Church", Path::new("godwinmix.toml"), Path::new("/tmp/x"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("slug"), "{e}");
    }
}
