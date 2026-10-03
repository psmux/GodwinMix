//! Keeping the plugins somebody installed from this app up to date with it.
//!
//! Every first party plugin that is not a device plugin travels in the app as
//! `resources/plugins/<name>`, and the mixer installs it into the operator's
//! plugins directory the first time somebody picks its feature (see the core's
//! `first_party`). That copy is the mixer's, not this app's, so `plugins::seed`
//! never looked at it, and an upgraded app went on running the NDI or Icecast
//! plugin from the version it replaced.
//!
//! Here, at every launch and before the mixer starts, each installed copy whose
//! trust record says it came from this app's own plugin folder is compared with
//! what the app carries now, and replaced when they differ. A plugin the
//! operator installed from anywhere else is never touched.

use std::fs;
use std::io;
use std::path::Path;

use crate::plugins::{copy_tree, stamp_of, STAMP};

/// Refresh every installed plugin that came from `shipped`, the app's
/// `resources/plugins` folder, in `dest`, the operator's plugins directory.
pub fn refresh(shipped: &Path, dest: &Path) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(shipped) else { return Ok(()) };
    for entry in entries.flatten() {
        let from = entry.path();
        let Some(version) = version_of(&from) else { continue };
        let name = entry.file_name().to_string_lossy().into_owned();
        refresh_one(&name, &version, &from, &dest.join(&name))?;
    }
    Ok(())
}

fn refresh_one(name: &str, version: &str, from: &Path, name_dir: &Path) -> io::Result<()> {
    let Ok(installed) = fs::read_dir(name_dir) else { return Ok(()) };
    let stamp = stamp_of(from);
    let ours: Vec<_> = installed
        .flatten()
        .map(|e| e.path())
        .filter(|dir| came_from(dir, from))
        .collect();
    if ours.is_empty() {
        // Not installed, or installed from somewhere else. Nothing runs
        // until it is asked for, and theirs wins.
        return Ok(());
    }
    let into = name_dir.join(version);
    let current = fs::read_to_string(into.join(STAMP)).ok();
    if ours.contains(&into) && current.as_deref().map(str::trim) == Some(stamp.trim()) {
        return Ok(());
    }
    for dir in &ours {
        fs::remove_dir_all(dir)?;
    }
    copy_tree(from, &into)?;
    fs::write(into.join(STAMP), &stamp)?;
    fs::write(into.join(".gmx-trust.json"), trust_record(from))?;
    eprintln!("[desktop] brought {name} up to {version} from {}", from.display());
    Ok(())
}

/// Whether the copy in `dir` was installed from `from`, by its trust record.
fn came_from(dir: &Path, from: &Path) -> bool {
    let Ok(text) = fs::read_to_string(dir.join(".gmx-trust.json")) else { return false };
    let Ok(record) = serde_json::from_str::<serde_json::Value>(&text) else { return false };
    let source = record.get("source").and_then(|s| s.as_str()).unwrap_or_default();
    same_place(Path::new(source), from)
}

/// Two spellings of one directory: `\\?\` and case are not differences on
/// Windows.
fn same_place(a: &Path, b: &Path) -> bool {
    let plain = |p: &Path| {
        let s = p.to_string_lossy().replace('/', "\\");
        let s = s.strip_prefix(r"\\?\").unwrap_or(&s).trim_end_matches('\\').to_string();
        if cfg!(windows) { s.to_lowercase() } else { s }
    };
    plain(a) == plain(b)
}

/// The record the mixer writes for a plugin that ships with it.
fn trust_record(from: &Path) -> String {
    serde_json::json!({ "source": from.display().to_string(), "shipped": true }).to_string()
}

/// `[plugin] version` from a plugin directory's manifest, read as text: the
/// app has no TOML parser and needs one line.
fn version_of(dir: &Path) -> Option<String> {
    let text = fs::read_to_string(dir.join("gmx-plugin.toml")).ok()?;
    let mut in_plugin = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            in_plugin = line == "[plugin]";
            continue;
        }
        if let Some(rest) = line.strip_prefix("version").filter(|_| in_plugin) {
            let value = rest.trim_start().strip_prefix('=')?.trim();
            return Some(value.trim_matches('"').to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(dir: &Path, version: &str, body: &str) {
        fs::create_dir_all(dir.join("bin")).unwrap();
        fs::write(
            dir.join("gmx-plugin.toml"),
            format!("[plugin]\nname = \"x\"\nversion = \"{version}\"\n\n[run]\nversion = \"no\"\n"),
        )
        .unwrap();
        fs::write(dir.join("bin").join("x"), body).unwrap();
    }

    #[test]
    fn a_copy_installed_from_the_app_follows_the_app_and_others_are_left() {
        let root = std::env::temp_dir().join(format!("gmx-shipped-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (shipped, dest) = (root.join("app").join("plugins"), root.join("data"));
        plugin(&shipped.join("ndi"), "0.2.0", "new build");
        plugin(&shipped.join("udp"), "0.1.0", "new udp");
        // ndi was installed from the app when it carried 0.1.0.
        let old = dest.join("ndi").join("0.1.0");
        plugin(&old, "0.1.0", "old build");
        let source = format!(r"\\?\{}", shipped.join("ndi").display());
        fs::write(old.join(".gmx-trust.json"), serde_json::json!({ "source": source }).to_string()).unwrap();
        // udp was installed by hand from somewhere else.
        let theirs = dest.join("udp").join("0.1.0");
        plugin(&theirs, "0.1.0", "their build");
        fs::write(theirs.join(".gmx-trust.json"), r#"{"source":"C:\\mine\\udp"}"#).unwrap();

        refresh(&shipped, &dest).unwrap();
        assert!(!old.exists(), "the old version went");
        let new = dest.join("ndi").join("0.2.0");
        assert_eq!(fs::read_to_string(new.join("bin").join("x")).unwrap(), "new build");
        assert!(fs::read_to_string(new.join(".gmx-trust.json")).unwrap().contains("\"shipped\":true"));
        assert_eq!(fs::read_to_string(theirs.join("bin").join("x")).unwrap(), "their build");
        // Nothing to do the second time.
        refresh(&shipped, &dest).unwrap();
        assert!(new.join(STAMP).is_file());
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn the_version_comes_from_the_plugin_table_only() {
        let dir = std::env::temp_dir().join(format!("gmx-shipped-v-{}", std::process::id()));
        plugin(&dir, "1.2.3", "");
        assert_eq!(version_of(&dir).as_deref(), Some("1.2.3"));
        let _ = fs::remove_dir_all(&dir);
    }
}
