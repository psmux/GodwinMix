//! Bringing a file, a folder or a zip of them into the library.
//!
//! Each file that decodes, or that is a gl-transitions shader, becomes a
//! folder of its own under `fx/`: a copy of the file, the manifest the
//! measurement wrote, and the preview strip. A folder that already has an
//! `graphic.toml` is copied as it is. A licence or read me found beside the files
//! is copied into every item made from them, and its first line is kept in
//! the manifest, so the terms travel with the file.

use super::{library, sprite};
use anyhow::{bail, Context, Result};
use godwinmix_protocol::fx::{FxImportRequest, FxImported, FxSkipped};
use std::path::{Path, PathBuf};

const MEDIA: &[&str] = &["webm", "mov", "mp4", "m4v", "mkv", "avi", "gif", "png", "jpg", "jpeg", "tif", "tiff", "bmp", "webp"];
const SHADERS: &[&str] = &["glsl", "frag", "fs"];

/// Import what `req.path` names into the library under `media`.
pub fn import(media: &Path, req: &FxImportRequest) -> Result<FxImported> {
    let path = locate(media, &req.path)?;
    let root = library::root(media);
    std::fs::create_dir_all(&root).with_context(|| format!("making {}", root.display()))?;
    let mut done = FxImported::default();
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("zip")) {
        let unpacked = unzip(&path, &root)?;
        let result = folder(&unpacked, &root, req, &mut done);
        let _ = std::fs::remove_dir_all(&unpacked);
        result?;
    } else if path.is_dir() {
        folder(&path, &root, req, &mut done)?;
    } else {
        let licence = licence_beside(&path);
        match file(&path, &root, req, req.name.as_deref(), licence.as_ref()) {
            Ok(entry) => done.imported.push(entry),
            Err(e) => bail!("{} was not imported: {e:#}", path.display()),
        }
    }
    Ok(done)
}

/// An absolute path, or one inside the media folder.
fn locate(media: &Path, asked: &str) -> Result<PathBuf> {
    let p = Path::new(asked.trim());
    let path = if p.is_absolute() {
        p.to_path_buf()
    } else {
        if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            bail!("{asked:?} climbs out of the media folder. Give an absolute path, or a name inside the media folder");
        }
        media.join(p)
    };
    if !path.exists() {
        bail!("there is nothing at {}. Give an absolute path on the mixer's machine, or upload the file with media.upload and give its name", path.display());
    }
    Ok(path)
}

/// Every item in a folder: folders with a `graphic.toml` as they are, and every
/// media file or shader, two levels down.
fn folder(dir: &Path, root: &Path, req: &FxImportRequest, done: &mut FxImported) -> Result<()> {
    for path in walk(dir, 3) {
        let skip = |reason: String| FxSkipped { file: path.display().to_string(), reason };
        if path.is_dir() {
            match copy_item(&path, root, req.replace) {
                Ok(entry) => done.imported.push(entry),
                Err(e) => done.skipped.push(skip(format!("{e:#}"))),
            }
            continue;
        }
        let near = path.ancestors().skip(1).take_while(|a| a.starts_with(dir)).find_map(licence_in);
        match file(&path, root, req, None, near.as_ref()) {
            Ok(entry) => done.imported.push(entry),
            Err(e) => done.skipped.push(skip(format!("{e:#}"))),
        }
    }
    Ok(())
}

/// Media files, shaders, and folders holding a `graphic.toml`, in name order.
fn walk(dir: &Path, depth: usize) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for p in paths {
        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        if name.starts_with('.') || name.starts_with("__MACOSX") {
            continue;
        }
        if p.is_dir() {
            if p.join(library::MANIFEST).is_file() {
                out.push(p);
            } else if depth > 1 {
                out.extend(walk(&p, depth - 1));
            }
        } else if wanted(&p) {
            out.push(p);
        }
    }
    out
}

fn ext(p: &Path) -> String {
    p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

fn wanted(p: &Path) -> bool {
    let e = ext(p);
    MEDIA.contains(&e.as_str()) || SHADERS.contains(&e.as_str())
}

/// One file, measured and written in as an item.
fn file(path: &Path, root: &Path, req: &FxImportRequest, name: Option<&str>, licence: Option<&(PathBuf, String)>) -> Result<godwinmix_protocol::fx::FxEntry> {
    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    let mut slug = library::slug(name.unwrap_or(&stem));
    if godwinmix_protocol::requests::TRANSITIONS.contains(&slug.as_str()) {
        slug.push_str("-fx");
    }
    let dir = root.join(&slug);
    if dir.exists() && !req.replace {
        bail!("the library already has {slug}. Import it under another name, or pass replace: true");
    }
    let file_name = format!("{slug}.{}", ext(path));
    let (manifest, measured) = describe(path, &slug, &file_name, req)?;
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    std::fs::copy(path, dir.join(&file_name)).with_context(|| format!("copying {}", path.display()))?;
    let mut manifest = manifest;
    if let Some((file, first)) = licence {
        let _ = std::fs::copy(file, dir.join(file.file_name().unwrap_or_default()));
        manifest.licence = Some(first.clone());
    }
    library::save(&dir, &manifest)?;
    if let Ok(pictures) = sprite::render(&manifest, &dir, measured.as_ref()) {
        let _ = sprite::write(&dir, pictures);
    }
    Ok(library::entry(&manifest, &dir))
}

#[path = "import_describe.rs"]
mod describe;
use describe::{copy_item, describe, licence_beside, licence_in, unzip};
