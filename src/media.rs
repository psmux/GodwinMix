//! The ad library: media files on the machine running the mixer.
//!
//! Deliberately server side. A browser file picker hands back a file from the
//! operator's own machine with no usable path, and the mixer needs something it
//! can open itself. Listing a directory on the server means the same UI works
//! whether the operator is sitting at the machine or on the other side of the
//! country, which is the property the whole control plane is built around.

use crate::config::MediaConfig;
use anyhow::{Context, Result};
use gstreamer as gst;
use gstreamer_pbutils::Discoverer;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, warn};

/// Containers worth offering as an ad. Anything else in the directory is
/// ignored rather than listed and then failing when someone clicks it.
const EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "ts", "mpg", "mpeg", "flv", "wmv",
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaItem {
    /// Name shown in the UI, relative to the library root.
    pub name: String,
    /// Absolute path, which is what gets handed back to the ad break API.
    pub path: String,
    pub size_bytes: u64,
    /// None when the file could not be inspected; it is still listed, because
    /// an operator would rather see a clip they cannot read the length of than
    /// wonder why it is missing.
    pub duration_ms: Option<u64>,
    pub has_video: bool,
    pub has_audio: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaListing {
    pub dir: String,
    pub items: Vec<MediaItem>,
    /// Set when the directory itself could not be read, so the UI can say why
    /// the list is empty instead of just showing nothing.
    pub error: Option<String>,
}

/// Cached probe result, keyed by path and invalidated when the file changes.
#[derive(Clone)]
struct Probed {
    modified: Option<SystemTime>,
    size: u64,
    duration_ms: Option<u64>,
    has_video: bool,
    has_audio: bool,
}

pub struct MediaLibrary {
    cfg: MediaConfig,
    cache: Mutex<HashMap<PathBuf, Probed>>,
}

impl MediaLibrary {
    pub fn new(cfg: MediaConfig) -> Self {
        Self { cfg, cache: Mutex::new(HashMap::new()) }
    }

    pub fn dir(&self) -> &Path {
        Path::new(&self.cfg.dir)
    }

    /// Scan the library. Blocking: inspecting a file opens and demuxes it, so
    /// callers run this off the async runtime's worker threads.
    pub fn list(&self) -> MediaListing {
        let dir = self.dir().to_path_buf();
        let mut items = Vec::new();
        let mut error = None;

        match self.walk(&dir, &dir, 0, &mut items) {
            Ok(()) => {}
            Err(e) => {
                warn!(dir = %dir.display(), ?e, "could not read the media library");
                error = Some(format!("{e:#}"));
            }
        }
        items.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        MediaListing { dir: dir.display().to_string(), items, error }
    }

    fn walk(
        &self,
        root: &Path,
        dir: &Path,
        depth: usize,
        out: &mut Vec<MediaItem>,
    ) -> Result<()> {
        if depth > self.cfg.max_depth || out.len() >= self.cfg.max_files {
            return Ok(());
        }
        let entries = std::fs::read_dir(dir)
            .with_context(|| format!("reading {}", dir.display()))?;

        for entry in entries.flatten() {
            if out.len() >= self.cfg.max_files {
                break;
            }
            let path = entry.path();
            // Do not follow symlinks: a link pointing outside the library would
            // quietly widen what the control port exposes.
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_symlink() {
                continue;
            }
            if meta.is_dir() {
                let _ = self.walk(root, &path, depth + 1, out);
                continue;
            }
            if !is_media(&path) {
                continue;
            }
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .display()
                .to_string();
            let probed = self.probe(&path, &meta);
            out.push(MediaItem {
                name,
                path: path.display().to_string(),
                size_bytes: meta.len(),
                duration_ms: probed.duration_ms,
                has_video: probed.has_video,
                has_audio: probed.has_audio,
            });
        }
        Ok(())
    }

    /// Inspect a file, reusing the cached answer while it is unchanged.
    fn probe(&self, path: &Path, meta: &std::fs::Metadata) -> Probed {
        let modified = meta.modified().ok();
        if let Some(hit) = self.cache.lock().get(path) {
            if hit.modified == modified && hit.size == meta.len() {
                return hit.clone();
            }
        }

        let probed = discover(path, self.cfg.probe_timeout_secs).unwrap_or_else(|e| {
            debug!(path = %path.display(), ?e, "could not inspect media file");
            Probed { modified, size: meta.len(), duration_ms: None, has_video: false, has_audio: false }
        });
        let probed = Probed { modified, size: meta.len(), ..probed };
        self.cache.lock().insert(path.to_path_buf(), probed.clone());
        probed
    }
}

fn is_media(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

fn discover(path: &Path, timeout_secs: u64) -> Result<Probed> {
    let uri = crate::input::to_uri(&path.display().to_string());
    let d = Discoverer::new(gst::ClockTime::from_seconds(timeout_secs.max(1)))
        .context("creating discoverer")?;
    let info = d.discover_uri(&uri).context("inspecting file")?;
    Ok(Probed {
        modified: None,
        size: 0,
        duration_ms: info.duration().map(|d| d.mseconds()),
        has_video: !info.video_streams().is_empty(),
        has_audio: !info.audio_streams().is_empty(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_media_extensions_are_offered() {
        assert!(is_media(Path::new("/x/ad.mp4")));
        assert!(is_media(Path::new("/x/AD.MP4")));
        assert!(is_media(Path::new("/x/clip.mov")));
        assert!(!is_media(Path::new("/x/notes.txt")));
        assert!(!is_media(Path::new("/x/no-extension")));
    }

    #[test]
    fn a_missing_library_reports_why_rather_than_looking_empty() {
        let _ = gst::init();
        let lib = MediaLibrary::new(MediaConfig {
            dir: "/definitely/not/here".into(),
            ..Default::default()
        });
        let listing = lib.list();
        assert!(listing.items.is_empty());
        assert!(listing.error.is_some(), "an unreadable directory must say so");
    }

    #[test]
    fn lists_media_and_ignores_everything_else() {
        let _ = gst::init();
        let dir = std::env::temp_dir().join(format!("lbx-media-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sub"));
        std::fs::write(dir.join("b.mp4"), b"not really a video").unwrap();
        std::fs::write(dir.join("a.mov"), b"nor this").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignore me").unwrap();
        std::fs::write(dir.join("sub").join("c.mkv"), b"nested").unwrap();

        let lib = MediaLibrary::new(MediaConfig {
            dir: dir.display().to_string(),
            ..Default::default()
        });
        let listing = lib.list();
        let names: Vec<_> = listing.items.iter().map(|i| i.name.clone()).collect();
        assert!(listing.error.is_none());
        assert_eq!(names, vec!["a.mov", "b.mp4", "sub/c.mkv"], "sorted, nested, text excluded");
        // Unreadable files are still listed, just without a duration.
        assert!(listing.items.iter().all(|i| i.duration_ms.is_none()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
