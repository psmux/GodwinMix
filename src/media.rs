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
    /// Short codec name of the first video stream ("h264", "vp9"), None when
    /// there is no video or it could not be inspected.
    #[serde(default)]
    pub video_codec: Option<String>,
    #[serde(default)]
    pub audio_codec: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    /// True when the file needs no conversion to play in a browser: H.264 plus
    /// AAC (or no audio) in an MP4. See `convert::web_safety`.
    #[serde(default)]
    pub web_safe: bool,
    /// Why it is not, in words an operator can act on. Empty when it is.
    #[serde(default)]
    pub reasons: Vec<String>,
    /// The converted copy's absolute path when one exists on disk. This is
    /// what "add as source" should prefer over `path`.
    /// Whether the moov atom comes first (a player can start before the whole
    /// file arrives). None for a non-ISO container, never a reason to convert
    /// on its own. See `convert::moov_first`.
    #[serde(default)]
    pub faststart: Option<bool>,
    #[serde(default)]
    pub converted_path: Option<String>,
    /// Where a conversion of this file stands, None when none was asked for in
    /// this process's lifetime.
    #[serde(default)]
    pub conversion: Option<crate::convert::ConversionState>,
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
    video_codec: Option<String>,
    audio_codec: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    web_safe: bool,
    reasons: Vec<String>,
    faststart: Option<bool>,
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

    pub fn cfg(&self) -> &MediaConfig {
        &self.cfg
    }

    /// The real path a listed name refers to, or an error. Names come back
    /// from `list` with `/` separators and may name a subdirectory, so this
    /// cannot simply refuse every slash the way an upload does. It checks each
    /// component, then confirms the canonical result really is inside the
    /// library, which is the check that survives a symlink planted by hand.
    pub fn resolve(&self, name: &str) -> Result<PathBuf> {
        anyhow::ensure!(!name.trim().is_empty(), "which file?");
        let mut out = self.dir().to_path_buf();
        for part in name.split('/') {
            anyhow::ensure!(
                !part.is_empty()
                    && part != "."
                    && part != ".."
                    && !part.contains('\\')
                    && !part.chars().any(char::is_control),
                "{name:?} is not a name from this library",
            );
            out.push(part);
        }
        let root = self.dir().canonicalize().context("the media library is not readable")?;
        let real = out.canonicalize().with_context(|| format!("no such file {name}"))?;
        anyhow::ensure!(real.starts_with(&root), "{name:?} is outside the media library");
        anyhow::ensure!(real.is_file() && is_media(&real), "{name:?} is not a clip");
        Ok(real)
    }

    /// Scan the library, attaching each file's conversion state from the
    /// converter when one is given. Blocking: inspecting a file opens and
    /// demuxes it, so callers run this off the async runtime's worker threads.
    pub fn list_with(&self, converter: Option<&crate::convert::Converter>) -> MediaListing {
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

        // Fold a `clip.web.mp4` onto `clip.<ext>` as its converted copy, rather
        // than listing it as its own file. `converted_stem` strips `.web.mp4`;
        // `plain_stem` strips one ordinary extension. A `.web.mp4` whose
        // original is not in the library is left in the list, because someone
        // may have uploaded one directly.
        let originals: std::collections::HashSet<String> =
            items.iter().filter(|i| !crate::convert::is_converted_name(&i.name)).map(|i| plain_stem(&i.name)).collect();
        let converted: std::collections::HashMap<String, String> = items
            .iter()
            .filter(|i| crate::convert::is_converted_name(&i.name) && originals.contains(&converted_stem(&i.name)))
            .map(|i| (converted_stem(&i.name), i.path.clone()))
            .collect();
        items.retain(|i| {
            !(crate::convert::is_converted_name(&i.name) && converted.contains_key(&converted_stem(&i.name)))
        });
        for it in items.iter_mut() {
            if let Some(path) = converted.get(&plain_stem(&it.name)) {
                it.converted_path = Some(path.clone());
            }
            if let Some(conv) = converter {
                it.conversion = conv.state(&it.name);
            }
        }
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
            // A partial upload or conversion writes a dotted or `.part` name.
            // Never list one: it is selectable and taking it plays a truncated
            // clip.
            if entry.file_name().to_string_lossy().starts_with('.')
                || path.extension().and_then(|e| e.to_str()) == Some("part")
            {
                continue;
            }
            // Spelled with `/` on every platform. The name is how the API and
            // an ad break refer to the file, and a name that reads `sub\c.mkv`
            // on one machine and `sub/c.mkv` on another is two names.
            let name = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let probed = self.probe(&path, &meta);
            out.push(MediaItem {
                name,
                path: path.display().to_string(),
                size_bytes: meta.len(),
                duration_ms: probed.duration_ms,
                has_video: probed.has_video,
                has_audio: probed.has_audio,
                video_codec: probed.video_codec.clone(),
                audio_codec: probed.audio_codec.clone(),
                width: probed.width,
                height: probed.height,
                web_safe: probed.web_safe,
                reasons: probed.reasons.clone(),
                faststart: probed.faststart,
                converted_path: None,
                conversion: None,
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
            Probed {
                modified,
                size: meta.len(),
                duration_ms: None,
                has_video: false,
                has_audio: false,
                video_codec: None,
                audio_codec: None,
                width: None,
                height: None,
                web_safe: false,
                reasons: vec!["could not inspect this file".into()],
                faststart: None,
            }
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
    let safety = crate::convert::web_safety(&info);
    Ok(Probed {
        faststart: crate::convert::moov_first(path),
        modified: None,
        size: 0,
        duration_ms: info.duration().map(|d| d.mseconds()),
        has_video: !info.video_streams().is_empty(),
        has_audio: !info.audio_streams().is_empty(),
        video_codec: safety.video_codec,
        audio_codec: safety.audio_codec,
        width: safety.width,
        height: safety.height,
        web_safe: safety.safe,
        reasons: safety.reasons,
    })
}

/// The stem of an ordinary file name, one extension stripped: `sub/clip.mkv`
/// gives `sub/clip`.
fn plain_stem(name: &str) -> String {
    match name.rfind('.') {
        Some(i) if !name[i..].contains('/') => name[..i].to_string(),
        _ => name.to_string(),
    }
}

/// The stem of a converted name, `.web.mp4` stripped: `sub/clip.web.mp4` gives
/// `sub/clip`.
fn converted_stem(name: &str) -> String {
    name.strip_suffix(".web.mp4").unwrap_or(name).to_string()
}

/// A name an upload may write: one segment, a known media extension, no tricks.
/// Flat on purpose. A caller that can name a path inside a subdirectory can
/// also name `..`, and the whole point of this directory is that its contents
/// get opened and played.
pub fn safe_upload_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    anyhow::ensure!(!name.is_empty(), "an upload needs a file name");
    anyhow::ensure!(name.len() <= 200, "that file name is too long");
    anyhow::ensure!(
        !name.contains(['/', '\\', '\0']) && name != "." && name != "..",
        "a file name is one segment: no slashes, no ..",
    );
    anyhow::ensure!(!name.starts_with('.'), "a name starting with a dot is hidden from the library");
    anyhow::ensure!(!name.chars().any(|c| c.is_control()), "a file name cannot contain control characters");
    anyhow::ensure!(
        is_media(Path::new(name)),
        "only video containers are accepted here: {}",
        EXTENSIONS.join(", "),
    );
    Ok(name.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_upload_name_is_one_segment_with_a_known_extension() {
        assert!(safe_upload_name("clip.mp4").is_ok());
        assert!(safe_upload_name("  Sting.MOV  ").is_ok(), "trimmed and case insensitive");
        assert!(safe_upload_name("../clip.mp4").is_err());
        assert!(safe_upload_name("a/b.mp4").is_err(), "no subdirectories on upload");
        assert!(safe_upload_name("a\\b.mp4").is_err());
        assert!(safe_upload_name(".hidden.mp4").is_err());
        assert!(safe_upload_name("notes.txt").is_err(), "not a video container");
        assert!(safe_upload_name("").is_err());
    }

    #[test]
    fn a_listed_name_resolves_inside_the_library_and_nowhere_else() {
        let dir = std::env::temp_dir().join(format!("gmx-resolve-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sub"));
        std::fs::write(dir.join("a.mp4"), b"x").unwrap();
        std::fs::write(dir.join("sub").join("c.mkv"), b"x").unwrap();
        let lib = MediaLibrary::new(MediaConfig { dir: dir.display().to_string(), ..Default::default() });

        assert!(lib.resolve("a.mp4").is_ok());
        assert!(lib.resolve("sub/c.mkv").is_ok(), "a nested listed name resolves");
        assert!(lib.resolve("../../etc/passwd").is_err());
        assert!(lib.resolve("sub/../../../etc/passwd").is_err());
        assert!(lib.resolve("nope.mp4").is_err(), "a file that is not there");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_converted_copy_folds_onto_its_original() {
        let _ = gst::init();
        let dir = std::env::temp_dir().join(format!("gmx-fold-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        std::fs::write(dir.join("clip.mkv"), b"x").unwrap();
        std::fs::write(dir.join("clip.web.mp4"), b"x").unwrap();
        std::fs::write(dir.join("orphan.web.mp4"), b"x").unwrap();
        let lib = MediaLibrary::new(MediaConfig { dir: dir.display().to_string(), ..Default::default() });
        let listing = lib.list_with(None);
        let names: Vec<_> = listing.items.iter().map(|i| i.name.clone()).collect();
        assert!(names.contains(&"clip.mkv".to_string()), "the original is listed");
        assert!(!names.contains(&"clip.web.mp4".to_string()), "its converted copy is folded away");
        assert!(names.contains(&"orphan.web.mp4".to_string()), "a converted file with no original stays");
        let clip = listing.items.iter().find(|i| i.name == "clip.mkv").unwrap();
        assert!(clip.converted_path.is_some(), "the original points at its converted copy");
        let _ = std::fs::remove_dir_all(&dir);
    }

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
        let listing = lib.list_with(None);
        assert!(listing.items.is_empty());
        assert!(listing.error.is_some(), "an unreadable directory must say so");
    }

    #[test]
    fn lists_media_and_ignores_everything_else() {
        let _ = gst::init();
        let dir = std::env::temp_dir().join(format!("gmx-media-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("sub"));
        std::fs::write(dir.join("b.mp4"), b"not really a video").unwrap();
        std::fs::write(dir.join("a.mov"), b"nor this").unwrap();
        std::fs::write(dir.join("notes.txt"), b"ignore me").unwrap();
        std::fs::write(dir.join("sub").join("c.mkv"), b"nested").unwrap();

        let lib = MediaLibrary::new(MediaConfig {
            dir: dir.display().to_string(),
            ..Default::default()
        });
        let listing = lib.list_with(None);
        let names: Vec<_> = listing.items.iter().map(|i| i.name.clone()).collect();
        assert!(listing.error.is_none());
        assert_eq!(names, vec!["a.mov", "b.mp4", "sub/c.mkv"], "sorted, nested, text excluded");
        // Unreadable files are still listed, just without a duration.
        assert!(listing.items.iter().all(|i| i.duration_ms.is_none()));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
