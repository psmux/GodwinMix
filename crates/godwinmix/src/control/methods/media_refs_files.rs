//! The files that can name a library picture by its path: an SVG or HTML
//! template in the library (`href="Sermons/logo.png"`, `media:logo.png`),
//! and a Graphics gallery item's `graphic.toml` (an image field's value).

use super::media_refs_map::Mapping;
use std::path::{Path, PathBuf};

const TEMPLATES: &[&str] = &["svg", "html", "htm"];

/// Rewrite each such file that names a moved file. Answers the ones
/// changed, and the ones that could not be written, in words.
pub fn rewrite(library: &Path, gallery: &Path, depth: usize, map: &Mapping) -> Vec<String> {
    let mut files = Vec::new();
    walk(library, depth, &mut files);
    if let Ok(items) = std::fs::read_dir(gallery) {
        files.extend(items.flatten().map(|e| e.path().join("graphic.toml")).filter(|p| p.is_file()));
    }
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        let Some(next) = map.text(&text) else { continue };
        let shown = f.strip_prefix(library).or_else(|_| f.strip_prefix(gallery)).unwrap_or(&f).display().to_string().replace('\\', "/");
        match godwinmix_core::config::edit::write_atomic(&f, next.as_bytes()) {
            Ok(()) => out.push(format!("file {shown}")),
            Err(e) => out.push(format!("{shown} still names the old place: {e:#}. Change it by hand")),
        }
    }
    out
}

/// Every template under `dir`, not following links, not into a gallery.
fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        let path = e.path();
        if kind.is_dir() && !kind.is_symlink() && depth > 0 && !path.join(godwinmix_core::gallery::MARKER).is_file() {
            walk(&path, depth - 1, out);
        } else if kind.is_file() && path.extension().and_then(|x| x.to_str()).is_some_and(|x| TEMPLATES.contains(&x.to_lowercase().as_str())) {
            out.push(path);
        }
    }
}
