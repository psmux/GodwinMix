//! The config keys a project carries, read from the file and written back
//! through `config.set`, so comments survive and live keys apply at once.
//!
//! Only keys the file writes itself travel: a default is not a choice anybody
//! made. They split in two. Show keys (the picture, the programme encode, the
//! multiview, the safety rules) are the project. Machine keys (addresses,
//! folders, hardware, plugins, the token) belong to the computer, so an import
//! writes them only when asked, and the token only when secrets were exported.

use godwinmix_core::config::keys::KEYS;
use godwinmix_core::config::schema::{lookup, to_json};
use godwinmix_core::config::Config;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Sections that describe the show rather than the machine it runs on.
const SHOW_SECTIONS: &[&str] = &["canvas", "program", "multiview", "snapshot", "safety", "stall"];

/// True for a key that belongs to the show.
pub fn is_show_key(key: &str) -> bool {
    let section = key.split('.').next().unwrap_or(key);
    SHOW_SECTIONS.contains(&section) || key == "browser.overlay_fps"
}

/// What the file sets, split into show and machine keys. A missing or broken
/// file is an empty project config, not an error: sources and scenes are
/// still worth saving.
pub fn read(path: &Path, secrets: bool) -> (BTreeMap<String, Value>, BTreeMap<String, Value>) {
    let mut show = BTreeMap::new();
    let mut machine = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(path) else { return (show, machine) };
    let (Ok(table), Ok(cfg)) = (toml::from_str::<toml::Table>(&text), Config::from_toml(&text, "the config")) else {
        return (show, machine);
    };
    let tree = to_json(&cfg);
    for key in KEYS {
        if !written(&table, key.key) || (key.secret && !secrets) {
            continue;
        }
        let Some(value) = lookup(&tree, key.key).cloned() else { continue };
        if is_show_key(key.key) {
            show.insert(key.key.to_string(), value);
        } else {
            machine.insert(key.key.to_string(), value);
        }
    }
    (show, machine)
}

/// Whether the file writes this dotted key itself.
pub fn written(table: &toml::Table, dotted: &str) -> bool {
    let steps: Vec<&str> = dotted.split('.').collect();
    let mut at = table;
    for (n, step) in steps.iter().enumerate() {
        match at.get(*step) {
            Some(toml::Value::Table(inner)) if n + 1 < steps.len() => at = inner,
            Some(_) => return n + 1 == steps.len(),
            None => return false,
        }
    }
    false
}

/// The keys an import would write, each with what it is now.
///
/// Replace writes every key the file has that differs. Merge writes only the
/// keys this mixer's file does not set, the rule `preset.apply` follows: the
/// operator's own values win.
pub fn changes(
    path: &Path,
    incoming: &BTreeMap<String, Value>,
    replace: bool,
) -> Vec<(String, Value, Option<Value>)> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let table = toml::from_str::<toml::Table>(&text).unwrap_or_default();
    let tree = Config::from_toml(&text, "the config").map(|c| to_json(&c)).unwrap_or(Value::Null);
    incoming
        .iter()
        .filter(|(key, _)| godwinmix_core::config::keys::find(key).is_some())
        .filter_map(|(key, value)| {
            let now = lookup(&tree, key).cloned();
            let set_here = written(&table, key);
            let differs = now.as_ref() != Some(value);
            let wanted = if replace { differs } else { !set_here && differs };
            wanted.then(|| (key.clone(), value.clone(), now))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_keys_travel_and_machine_keys_are_kept_apart() {
        let dir = std::env::temp_dir().join(format!("gmx-project-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("godwinmix.toml");
        std::fs::write(&path, "# keep me\n[canvas]\nwidth = 640\nheight = 360\n[control]\nbind = \"0.0.0.0:9\"\ntoken = \"t0p\"\n").unwrap();
        let (show, machine) = read(&path, false);
        assert_eq!(show.get("canvas.width"), Some(&Value::from(640)));
        assert!(!show.contains_key("canvas.fps"), "a default is not carried: {show:?}");
        assert_eq!(machine.get("control.bind"), Some(&Value::from("0.0.0.0:9")));
        assert!(!machine.contains_key("control.token"), "no token without secrets");
        let (_, machine) = read(&path, true);
        assert_eq!(machine.get("control.token"), Some(&Value::from("t0p")));

        let mut incoming = BTreeMap::new();
        incoming.insert("canvas.width".to_string(), Value::from(1280));
        incoming.insert("canvas.fps".to_string(), Value::from(25));
        let merge: Vec<String> = changes(&path, &incoming, false).into_iter().map(|c| c.0).collect();
        assert_eq!(merge, vec!["canvas.fps"], "merge leaves what this mixer set");
        let replace = changes(&path, &incoming, true);
        assert_eq!(replace.len(), 2);
        std::fs::remove_dir_all(&dir).ok();
    }
}
