//! The panels of installed plugins, for `/plugins/index.json`.
//!
//! `gmx plugin add` puts a plugin at `<plugins>/<name>/<version>/`, and the
//! index used to look only at `<plugins>/<name>/ui/`, the folder a person
//! drops a panel into by hand. So a plugin installed from the gallery ran and
//! its panel never appeared, before a restart or after one. This reads every
//! live installed plugin as well: a `panel` provide names its file, and a
//! plugin with no `panel` provide still gets the `ui/panel.js` or
//! `ui/panel.html` convention. Read from disk at request time like the rest,
//! so a plugin installed a moment ago is listed on the page's next fetch,
//! and so the answer is the same from a station, whose show did the install
//! in a process of its own.

use godwinmix_core::plugin::loader::{self, Installed};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// Every panel the installed plugins under `root` declare, leaving out the
/// names in `skip`, which already have a hand placed `ui/` folder, and any
/// plugin this process knows is switched off.
pub(super) fn installed(root: &Path, skip: &[String]) -> Vec<Value> {
    let mut found = Vec::new();
    for plugin in on_disk(root) {
        let name = plugin.name().to_string();
        let off = loader::get(&name).is_some_and(|known| !known.live());
        if skip.contains(&name) || off || plugin.problem.is_some() {
            continue;
        }
        found.extend(panels_of(&plugin));
    }
    found
}

/// The newest version of every plugin installed under `root`.
fn on_disk(root: &Path) -> Vec<Installed> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
        if let Some(found) = newest(&entry.path()) {
            out.push(loader::read(&found, &Default::default()));
        }
    }
    out
}

/// The newest `<version>/` under one plugin's folder that has a manifest.
fn newest(dir: &Path) -> Option<PathBuf> {
    let numbers = |p: &Path| -> Vec<u64> {
        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        name.split(['-', '+']).next().unwrap_or("").split('.').map(|x| x.parse().unwrap_or(0)).collect()
    };
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| !p.file_name().is_some_and(|n| n.to_string_lossy().starts_with('.')))
        .filter(|p| p.join("gmx-plugin.toml").is_file())
        .max_by_key(|p| numbers(p))
}

/// One plugin's panels, as index entries.
pub(super) fn panels_of(plugin: &Installed) -> Vec<Value> {
    let name = plugin.name();
    let ui = plugin.root.join("ui");
    let named: serde_json::Map<String, Value> = std::fs::read_to_string(ui.join("panel.json"))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    let text = |key: &str, or: &str| named.get(key).and_then(Value::as_str).unwrap_or(or).to_string();
    let mut out = Vec::new();
    for provide in plugin.manifest.provides.iter().filter(|p| p.kind == "panel") {
        let Some(panel) = &provide.panel else { continue };
        let Some(inside) = panel.entry.strip_prefix("ui/").filter(|_| plugin.root.join(&panel.entry).is_file()) else {
            tracing::warn!(plugin = %name, entry = %panel.entry, "a panel entry is served only from the plugin's ui/ folder");
            continue;
        };
        let url = format!("/plugins/{name}/ui/{inside}");
        let trusted = panel.kind == "custom-element";
        out.push(json!({
            "name": name,
            "id": format!("{name}/{}", provide.id),
            "title": text("title", &provide.id),
            "slots": panel.slots,
            "height": named.get("height").cloned().unwrap_or_else(|| json!(260)),
            "tier": if trusted { "trusted" } else { "sandboxed" },
            "module": trusted.then(|| url.clone()),
            "page": (!trusted).then_some(url),
        }));
    }
    if !out.is_empty() {
        return out;
    }
    let module = ui.join("panel.js").is_file();
    let page = ui.join("panel.html").is_file();
    if module || page {
        out.push(json!({
            "name": name,
            "id": text("id", &format!("{name}/panel")),
            "title": text("title", name),
            "slots": named.get("slots").cloned().unwrap_or_else(|| json!(["sidebar"])),
            "height": named.get("height").cloned().unwrap_or_else(|| json!(260)),
            "tier": text("tier", if module { "trusted" } else { "sandboxed" }),
            "module": module.then(|| format!("/plugins/{name}/ui/panel.js")),
            "page": page.then(|| format!("/plugins/{name}/ui/panel.html")),
        }));
    }
    out
}

/// A file under an installed plugin's `ui/` folder, when it is there. `name`
/// is already a checked slug.
pub(super) fn file(root: &Path, name: &str, relative: &Path) -> Option<PathBuf> {
    let path = newest(&root.join(name))?.join("ui").join(relative);
    path.is_file().then_some(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin_at(dir: &std::path::Path, manifest: &str) -> Installed {
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join("main.py"), "").unwrap();
        std::fs::write(dir.join("gmx-plugin.toml"), manifest).unwrap();
        loader::read(dir, &Default::default())
    }

    const HEAD: &str = "[plugin]\nname = \"psalm\"\nversion = \"1.0.0\"\napi = 1\n\
        description = \"d\"\nlicense = \"MIT\"\nplatforms = [\"any\"]\nplacements = [\"sidecar\"]\n\
        [run]\npython = \"main.py\"\n";

    #[test]
    fn a_panel_provide_is_listed_from_the_versioned_folder_it_was_installed_to() {
        let dir = std::env::temp_dir().join(format!("gmx-panels-{}", std::process::id())).join("psalm/1.0.0");
        std::fs::create_dir_all(dir.join("ui")).unwrap();
        std::fs::write(dir.join("ui/verses.html"), "<p>hi</p>").unwrap();
        std::fs::write(dir.join("ui/panel.json"), r#"{"title":"Verses","height":300}"#).unwrap();
        let toml = format!(
            "{HEAD}[[provides]]\nkind = \"panel\"\nid = \"verses\"\n\
             panel = {{ kind = \"iframe\", entry = \"ui/verses.html\", slots = [\"sidebar\"] }}\n"
        );
        let plugin = plugin_at(&dir, &toml);
        assert!(plugin.problem.is_none(), "{:?}", plugin.problem);
        let found = panels_of(&plugin);
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!(found[0]["id"], "psalm/verses");
        assert_eq!(found[0]["title"], "Verses");
        assert_eq!(found[0]["page"], "/plugins/psalm/ui/verses.html");
        assert_eq!(found[0]["tier"], "sandboxed");
        assert!(found[0]["module"].is_null());
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).ok();
    }

    #[test]
    fn a_plugin_with_only_ui_panel_js_gets_the_folder_convention() {
        let dir = std::env::temp_dir().join(format!("gmx-panels-js-{}", std::process::id())).join("psalm/1.0.0");
        let plugin = plugin_at(&dir, HEAD);
        assert!(panels_of(&plugin).is_empty(), "nothing to show yet");
        std::fs::write(dir.join("ui/panel.js"), "export {}").unwrap();
        let found = panels_of(&plugin);
        assert_eq!(found[0]["module"], "/plugins/psalm/ui/panel.js");
        assert_eq!(found[0]["tier"], "trusted");
        assert_eq!(found[0]["id"], "psalm/panel");
        std::fs::remove_dir_all(dir.parent().unwrap().parent().unwrap()).ok();
    }
}
