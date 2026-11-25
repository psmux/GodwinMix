//! The scripts and the prompter's place on disk, beside the show's config.
//!
//! `godwinmix.toml` keeps them in `godwinmix.prompter.json`, so a show copied
//! with `show.add` takes its scripts with it (every file named for the
//! config's stem goes) and a restart brings them back, paused where the
//! prompter last stopped.

use super::{now_ms, Inner};
use godwinmix_protocol::prompter::{Look, Script};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Default, Serialize, Deserialize)]
pub(super) struct File {
    #[serde(default)]
    scripts: Vec<Script>,
    #[serde(default)]
    look: Option<Look>,
    #[serde(default)]
    on: Option<String>,
    #[serde(default)]
    position: f64,
    #[serde(default)]
    speed: Option<f64>,
}

impl File {
    pub(super) fn of(inner: &Inner) -> File {
        File {
            scripts: inner.scripts.values().cloned().collect(),
            look: Some(inner.look),
            on: inner.on.clone(),
            position: inner.position,
            speed: Some(inner.speed),
        }
    }
}

pub(super) fn path_beside(config: &Path) -> PathBuf {
    let stem = config.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "godwinmix".into());
    config.with_file_name(format!("{stem}.prompter.json"))
}

/// What was saved, or an empty prompter. A file that will not parse is moved
/// aside rather than overwritten, so nobody's script is lost to a bad edit.
pub(super) fn load(path: &Path) -> Inner {
    let mut inner = Inner::empty();
    let Ok(text) = std::fs::read_to_string(path) else { return inner };
    let file: File = match serde_json::from_str(&text) {
        Ok(f) => f,
        Err(e) => {
            let aside = path.with_extension("json.unreadable");
            let _ = std::fs::rename(path, &aside);
            tracing::error!(file = %path.display(), moved_to = %aside.display(), error = %e, "the prompter file would not parse; it was moved aside and this show starts with no scripts");
            return inner;
        }
    };
    inner.scripts = file.scripts.into_iter().map(|s| (s.id.clone(), s)).collect();
    inner.look = file.look.unwrap_or_default();
    inner.on = file.on.filter(|id| inner.scripts.contains_key(id));
    inner.speed = file.speed.unwrap_or(inner.speed);
    inner.position = file.position;
    inner.at_ms = now_ms();
    inner.rebuild();
    inner
}

/// Write the file, atomically.
pub(super) fn write(path: &Path, file: &File) {
    let text = serde_json::to_string_pretty(file).unwrap_or_default();
    let tmp = path.with_extension("json.tmp");
    let written = std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path));
    if let Err(e) = written {
        tracing::error!(file = %path.display(), error = %e, "the prompter scripts could not be saved; they last until a restart");
    }
}
