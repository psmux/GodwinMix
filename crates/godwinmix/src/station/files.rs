//! A new show's folder: a fresh config, or a copy of another show's.
//!
//! A copy takes every file a core keeps beside its config (the config, the
//! runtime store, the scenes), and not the channels, which are the
//! station's. Its outputs are left behind in both the config and the runtime
//! store, so a copy never sends a second stream to the same stream key.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub const CONFIG_NAME: &str = "godwinmix.toml";

/// Make `folder` with the config a first run writes, and answer its path.
pub fn fresh(folder: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    let path = folder.join(CONFIG_NAME);
    let body = godwinmix_core::config::first_run::first_run_config(godwinmix_core::config::first_run::EXAMPLE_CONFIG);
    godwinmix_core::config::edit::write_atomic(&path, body.as_bytes())?;
    Ok(path)
}

/// Copy the show whose config is `from` into `folder`, without its outputs.
pub fn copy(from: &Path, folder: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(folder).with_context(|| format!("making {}", folder.display()))?;
    let stem = from.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let dir = from.parent().unwrap_or(Path::new("."));
    let to = folder.join(CONFIG_NAME);
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_string();
        let beside = name.starts_with(&format!("{stem}.")) && !name.ends_with(".channels.toml") && !name.ends_with(".tmp");
        if !beside || !entry.file_type()?.is_file() {
            continue;
        }
        // Renamed to the new show's stem, so what the core looks for beside
        // `godwinmix.toml` is what it finds.
        let renamed = format!("godwinmix{}", &name[stem.len()..]);
        std::fs::copy(entry.path(), folder.join(&renamed)).with_context(|| format!("copying {name}"))?;
    }
    for file in [to.clone(), godwinmix_core::config::Config::runtime_store_path(&to)] {
        if file.exists() {
            godwinmix_core::config::edit::edit_file(&file, |doc| godwinmix_core::config::edit::remove(doc, "outputs"))?;
        }
    }
    Ok(to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_takes_the_scenes_and_leaves_the_outputs_and_the_channels_behind() {
        let dir = std::env::temp_dir().join(format!("gmx-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let from = dir.join("church.toml");
        std::fs::write(&from, "[canvas]\nwidth = 1280\n\n[[outputs]]\nid = \"yt\"\nuri = \"rtmp://a/b\"\n").unwrap();
        std::fs::write(dir.join("church.runtime.toml"), "[[outputs]]\nid = \"fb\"\nuri = \"rtmp://c/d\"\n").unwrap();
        std::fs::write(dir.join("church.runtime.scenes.json"), "{}").unwrap();
        std::fs::write(dir.join("church.runtime.channels.toml"), "").unwrap();
        let to = copy(&from, &dir.join("shows/copy")).unwrap();
        let config = std::fs::read_to_string(&to).unwrap();
        assert!(config.contains("width = 1280") && !config.contains("outputs"), "{config}");
        let runtime = std::fs::read_to_string(dir.join("shows/copy/godwinmix.runtime.toml")).unwrap();
        assert!(!runtime.contains("outputs"), "{runtime}");
        assert!(dir.join("shows/copy/godwinmix.runtime.scenes.json").exists());
        assert!(!dir.join("shows/copy/godwinmix.runtime.channels.toml").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
