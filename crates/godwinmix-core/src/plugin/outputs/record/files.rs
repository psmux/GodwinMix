//! A fresh file for every recording and reconnect. Existing footage is never replaced.
use crate::config::Params;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub fn settings(params: &Params) -> Result<(PathBuf, String)> {
    let format = params.get("format").map(|v| v.as_str().unwrap_or("")).unwrap_or("mp4");
    anyhow::ensure!(matches!(format, "mp4" | "mkv"), "record/output format must be mp4 or mkv; choose one before starting");
    let folder = match params.get("directory") {
        Some(value) => {
            let text = value.as_str().filter(|s| !s.trim().is_empty())
                .context("record/output directory is empty or not text; choose a writable folder on the mixer")?;
            PathBuf::from(text)
        }
        None => std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))
            .map(PathBuf::from).context("the mixer has no home directory; set params.directory")?
            .join("Videos").join("GodwinMix"),
    };
    Ok((folder, format.into()))
}

pub fn reserve(folder: &Path, id: &str, format: &str) -> Result<PathBuf> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    std::fs::create_dir_all(folder).with_context(|| format!("cannot create {}; choose a writable recording folder", folder.display()))?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis();
    let slug: String = id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    for _ in 0..100 {
        let n = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = folder.join(format!("{slug}-{stamp}-{n}.{format}"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(_) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e).with_context(|| format!("cannot write {}; choose a writable recording folder", path.display())),
        }
    }
    anyhow::bail!("cannot reserve a new recording name; choose another folder and start again")
}
