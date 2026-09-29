//! Where channels live on disk.
//!
//! `<stem>.channels.toml` beside the runtime store, the way the scene
//! collection sits beside it as `<stem>.scenes.json`. Beside rather than
//! inside, because the runtime store is rewritten whole by the mixer thread
//! every time a source changes, and two writers on one file is how a change
//! gets lost. Keys are not in it: only their ids, labels and hints. The keys
//! themselves are sealed in the secret store, the same one plugin secrets use.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use godwinmix_protocol::channels::KeyMode;
use serde::{Deserialize, Serialize};

/// One channel, as it is kept.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub app: String,
    #[serde(default = "yes")]
    pub enabled: bool,
    #[serde(default = "yes")]
    pub auto_source: bool,
    #[serde(default)]
    pub key_mode: KeyMode,
    #[serde(default)]
    pub keys: Vec<KeyRecord>,
    /// Sources this channel added, so a restart knows which are its own to
    /// take away again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub auto_sources: Vec<String>,
    /// Anything another part of the core keeps on a channel, such as its
    /// destinations, carried through untouched.
    #[serde(default, flatten)]
    pub extra: BTreeMap<String, toml::Value>,
}

fn yes() -> bool {
    true
}

/// A key without its secret.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyRecord {
    pub id: String,
    pub label: String,
    pub created: String,
    pub hint: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    channels: Vec<Record>,
}

/// The channels file's path, given the runtime store's.
pub fn path_beside(runtime_store: &Path) -> PathBuf {
    let stem = runtime_store.file_stem().map(|s| s.to_string_lossy().to_string());
    match stem {
        Some(stem) => runtime_store.with_file_name(format!("{stem}.channels.toml")),
        None => runtime_store.with_extension("channels.toml"),
    }
}

/// Read the channels, or none when there is no file yet. A file that will not
/// parse is an error rather than an empty list: saving over it would lose
/// somebody's channels.
pub fn load(path: &Path) -> Result<Vec<Record>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let file: File = toml::from_str(&text).with_context(|| format!("reading {}", path.display()))?;
    Ok(file.channels)
}

/// Write them, through a temporary file and a rename.
pub fn save(path: &Path, channels: &[Record]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).with_context(|| format!("making {}", dir.display()))?;
    }
    let body = toml::to_string_pretty(&File { channels: channels.to_vec() })
        .context("the channels would not serialise")?;
    let text = format!(
        "# RTMP channels, managed from the GodwinMix UI or API.\n\
         # Keys are sealed in the secret store; only their hints are here.\n\n{body}"
    );
    let temp = path.with_extension("toml.tmp");
    std::fs::write(&temp, text).with_context(|| format!("writing {}", temp.display()))?;
    std::fs::rename(&temp, path).with_context(|| format!("moving the channels into {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip_and_keep_what_another_module_wrote() {
        let dir = std::env::temp_dir().join(format!("gmx-channels-{}", std::process::id()));
        let path = path_beside(&dir.join("godwinmix.runtime.toml"));
        assert!(path.ends_with("godwinmix.runtime.channels.toml"));
        let mut record = Record {
            id: "sunday-service".into(),
            name: "Sunday service".into(),
            app: "sunday-service".into(),
            enabled: true,
            auto_source: true,
            key_mode: KeyMode::Query,
            keys: vec![KeyRecord {
                id: "obs".into(),
                label: "OBS".into(),
                created: "2026-09-29T10:00:00Z".into(),
                hint: "x7kq".into(),
            }],
            auto_sources: vec![],
            extra: BTreeMap::new(),
        };
        record.extra.insert("destinations".into(), toml::Value::Array(vec![]));
        save(&path, std::slice::from_ref(&record)).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(!text.contains("secret ="), "a key's secret is never written here: {text}");
        assert_eq!(load(&path).unwrap(), vec![record]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn no_file_is_no_channels_and_a_broken_one_is_an_error() {
        let dir = std::env::temp_dir().join(format!("gmx-channels-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.channels.toml");
        assert!(load(&path).unwrap().is_empty());
        std::fs::write(&path, "channels = 7").unwrap();
        assert!(load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
