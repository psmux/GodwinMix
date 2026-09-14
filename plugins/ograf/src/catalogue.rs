//! Finding the graphic a page asks for.
//!
//! A URL names `<plugin>/<provide>`. This resolves that against the plugins
//! directory (the parent of this plugin's own root), reads the plugin's
//! `gmx-plugin.toml`, finds the `graphic` provide and the OGraf manifest it
//! points at. So the host serves graphics from every plugin that ships them
//! and not only from its own, which is what makes it a host rather than a
//! player for one template.

use std::path::{Path, PathBuf};

/// One graphic, found on disk.
#[derive(Debug, Clone, PartialEq)]
pub struct Graphic {
    pub plugin: String,
    pub provide: String,
    /// The OGraf manifest, parsed.
    pub manifest: serde_json::Value,
    /// The directory the manifest is in. Everything the graphic loads is
    /// relative to it and may not climb out of it.
    pub dir: PathBuf,
}

impl Graphic {
    pub fn type_id(&self) -> String {
        format!("{}/{}", self.plugin, self.provide)
    }

    /// The module the web component is in, as the manifest names it.
    pub fn main(&self) -> &str {
        self.manifest
            .get("main")
            .and_then(|v| v.as_str())
            .unwrap_or("graphic.mjs")
    }

    /// The JSON Schema of the graphic's own data.
    pub fn schema(&self) -> serde_json::Value {
        self.manifest
            .get("schema")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({ "type": "object", "properties": {} }))
    }

    /// The defaults the schema declares, which is what a graphic shows before
    /// anybody has told it anything.
    pub fn defaults(&self) -> serde_json::Map<String, serde_json::Value> {
        let mut out = serde_json::Map::new();
        if let Some(properties) = self.schema().get("properties").and_then(|v| v.as_object()) {
            for (key, property) in properties {
                if let Some(default) = property.get("default") {
                    out.insert(key.clone(), default.clone());
                }
            }
        }
        out
    }
}

/// Where the plugins live: the parent of this plugin's own root.
pub fn plugins_dir(root: &Path) -> PathBuf {
    root.parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| root.to_path_buf())
}

/// Every graphic in the plugins directory, sorted, so the index page and a
/// picker list them the same way every time.
pub fn all(root: &Path) -> Vec<Graphic> {
    let dir = plugins_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().join("gmx-plugin.toml").is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    names.sort();
    let mut out = Vec::new();
    for name in names {
        out.extend(in_plugin(&dir.join(&name)));
    }
    out
}

/// Every graphic one plugin declares.
///
/// Through the SDK's own manifest parser rather than a hand rolled read of the
/// TOML: there is one definition of what a `gmx-plugin.toml` means, and a
/// graphics host that disagreed with it about a provide would serve a page the
/// core does not believe in.
pub fn in_plugin(root: &Path) -> Vec<Graphic> {
    let Ok(manifest) = godwinmix_sdk::manifest::Manifest::load(root.join("gmx-plugin.toml")) else {
        return Vec::new();
    };
    let plugin = manifest.plugin.name.clone();
    manifest
        .provides
        .iter()
        .filter(|p| p.kind == "graphic")
        .filter_map(|p| read(&plugin, &p.id, &root.join(p.graphic.as_ref()?)))
        .collect()
}

/// One graphic, by its plugin qualified id.
pub fn find(root: &Path, type_id: &str) -> Option<Graphic> {
    let (plugin, provide) = type_id.trim_matches('/').split_once('/')?;
    // The names come off a URL, so they are checked rather than joined: a
    // plugin called `../../etc` would otherwise read whatever it liked.
    if !is_name(plugin) || !is_name(provide) {
        return None;
    }
    in_plugin(&plugins_dir(root).join(plugin))
        .into_iter()
        .find(|g| g.provide == provide)
}

fn read(plugin: &str, provide: &str, at: &Path) -> Option<Graphic> {
    let text = std::fs::read_to_string(at).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(Graphic {
        plugin: plugin.to_string(),
        provide: provide.to_string(),
        manifest,
        dir: at.parent()?.to_path_buf(),
    })
}

/// A plugin or provide name: a slug, and nothing that walks a path.
pub fn is_name(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// Resolve a file the graphic asked for, refusing anything outside its own
/// directory.
///
/// Checked twice: once on the text, because `..` in a URL is the whole attack,
/// and once on the canonical path, because a symlink inside the directory is
/// the same attack with a longer fuse.
pub fn asset(graphic: &Graphic, relative: &str) -> Option<PathBuf> {
    if relative.is_empty() || relative.contains("..") || relative.starts_with('/') {
        return None;
    }
    let at = graphic.dir.join(relative);
    let real = at.canonicalize().ok()?;
    let base = graphic.dir.canonicalize().ok()?;
    real.starts_with(&base).then_some(real)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// This plugin's own root, whichever directory the test runs from.
    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    #[test]
    fn the_example_graphic_is_found_and_says_what_it_takes() {
        let found = in_plugin(&root());
        assert_eq!(found.len(), 1, "this plugin ships one graphic");
        let g = &found[0];
        assert_eq!(g.type_id(), "ograf/lower-third");
        assert_eq!(g.main(), "graphic.mjs");
        let schema = g.schema();
        let properties = schema["properties"]
            .as_object()
            .expect("a schema with properties");
        assert!(
            properties.contains_key("name"),
            "a lower third has a name in it"
        );
        assert!(properties.contains_key("title"));
        assert_eq!(g.manifest["stepCount"], 1);
    }

    #[test]
    fn its_defaults_are_what_it_shows_before_anybody_says_anything() {
        let g = &in_plugin(&root())[0];
        let defaults = g.defaults();
        assert!(defaults.contains_key("colour"), "{defaults:?}");
    }

    #[test]
    fn a_name_that_walks_out_of_the_plugins_directory_is_refused() {
        assert!(!is_name("../../etc"));
        assert!(!is_name(""));
        assert!(!is_name("a/b"));
        assert!(is_name("lower-third"));
        assert!(find(&root(), "../../etc/passwd").is_none());
    }

    #[test]
    fn a_graphic_may_only_load_files_from_its_own_directory() {
        let g = &in_plugin(&root())[0];
        assert!(asset(g, "graphic.mjs").is_some(), "its own module");
        assert!(
            asset(g, "../../../Cargo.toml").is_none(),
            "a path that climbs out"
        );
        assert!(asset(g, "/etc/passwd").is_none(), "an absolute path");
        assert!(
            asset(g, "nothing-here.js").is_none(),
            "a file that is not there"
        );
    }
}
