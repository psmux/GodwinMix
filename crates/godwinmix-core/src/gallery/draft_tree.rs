//! A draft from several files: a folder on disk, or a zip's members.
//!
//! A folder with a `graphic.toml` is an item as it stands. Without one, an
//! OGraf manifest makes an OGraf graphic, an `index.html` an HTML graphic,
//! and a folder holding one file is that file.

use super::{bytes, unknown, Draft, Refusal, MAX_BYTES};
use crate::gallery::manifest::Manifest;
use godwinmix_protocol::gallery::GalleryKind;
use std::collections::BTreeMap;
use std::path::Path;

/// Relative path with `/` to bytes.
pub type Files = BTreeMap<String, Vec<u8>>;

/// The most files one item may have.
const MAX_FILES: usize = 500;

/// `name` as a relative path with `/`, or `None` when it climbs out, is
/// absolute or names a drive.
pub fn clean(name: &str) -> Option<String> {
    let name = name.replace('\\', "/");
    let parts: Vec<&str> = name.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    let ok = !parts.is_empty()
        && !name.starts_with('/')
        && parts.iter().all(|p| *p != ".." && !p.contains(':') && !p.chars().any(char::is_control));
    ok.then(|| parts.join("/"))
}

/// The files of a folder or a zip.
pub fn from_tree(files: Files, name: &str) -> Result<Draft, Refusal> {
    let files = strip_top(files);
    if files.is_empty() {
        return Err(Refusal::new(format!("{name} is empty"), "Put the graphic's files in it and try again."));
    }
    if files.len() > MAX_FILES || files.values().map(Vec::len).sum::<usize>() > MAX_BYTES {
        return Err(Refusal::new(format!("{name} has more than {MAX_FILES} files or 512 MB"), "Import the items in it one at a time."));
    }
    if let Some(toml) = files.get(crate::gallery::MANIFEST) {
        return described(&files, toml, name);
    }
    let ograf = files.keys().find(|k| k.ends_with(".ograf.json") && !k.contains('/')).cloned();
    let (kind, main) = match (ograf, files.contains_key("index.html")) {
        (Some(main), _) => (GalleryKind::Ograf, main),
        (None, true) => (GalleryKind::Html, "index.html".to_string()),
        (None, false) if files.len() == 1 => {
            let (only, data) = files.into_iter().next().expect("one file");
            return bytes(&only, data);
        }
        _ => {
            return Err(Refusal::new(
                format!("{name} holds {} files and none of graphic.toml, index.html or a .ograf.json", files.len()),
                "Add a graphic.toml saying what it is (see docs/reference/gallery-format.md), or import the files one at a time.",
            ))
        }
    };
    if kind == GalleryKind::Ograf {
        serde_json::from_slice::<serde_json::Value>(&files[&main]).map_err(|e| {
            Refusal::new(format!("{main} is not JSON: {e}"), "Fix the OGraf manifest; the OGraf plugin's check script finds the line.")
        })?;
    }
    let manifest = Manifest { kind: kind.as_str().into(), file: main, ..Default::default() };
    Ok(Draft { manifest, files: files.into_iter().collect(), warnings: Vec::new() })
}

/// A folder that says what it is in its own `graphic.toml`.
fn described(files: &Files, toml: &[u8], name: &str) -> Result<Draft, Refusal> {
    let m = Manifest::parse(&String::from_utf8_lossy(toml), &format!("{name}/graphic.toml"))
        .map_err(|e| Refusal::new(format!("{e:#}"), "Fix graphic.toml as docs/reference/gallery-format.md describes, then import it again."))?;
    let needs_file = !matches!(m.kind(), Some(GalleryKind::Ticker | GalleryKind::Text | GalleryKind::Set));
    if needs_file && !files.contains_key(&m.file) {
        return Err(Refusal::new(
            format!("graphic.toml names file = {:?} and the folder has no such file", m.file),
            "Put the file beside graphic.toml, or correct the name in it.",
        ));
    }
    if let Some(set) = &m.set {
        if !files.contains_key(&set.background) {
            return Err(Refusal::new(format!("the set's background {:?} is not in the folder", set.background), "Put the picture beside graphic.toml."));
        }
    }
    let rest = files.iter().filter(|(k, _)| *k != crate::gallery::MANIFEST).map(|(k, v)| (k.clone(), v.clone())).collect();
    Ok(Draft { manifest: m, files: rest, warnings: Vec::new() })
}

/// Drop one folder every file is inside, as a zip of a folder has.
fn strip_top(files: Files) -> Files {
    let tops: std::collections::BTreeSet<&str> = files.keys().filter_map(|k| k.split_once('/').map(|(t, _)| t)).collect();
    let all_inside = files.keys().all(|k| k.contains('/'));
    match (tops.len(), all_inside) {
        (1, true) => {
            let top = format!("{}/", tops.into_iter().next().unwrap_or_default());
            files.into_iter().filter_map(|(k, v)| k.strip_prefix(&top).map(|s| (s.to_string(), v))).collect()
        }
        _ => files,
    }
}

/// A file or a folder on this machine.
pub fn from_path(path: &Path) -> Result<Draft, Refusal> {
    let shown = path.display().to_string();
    let missing = || Refusal::new(format!("there is no file or folder {shown} on the mixer's machine"), "Give the full path, or send the bytes as data with a filename.");
    let meta = std::fs::metadata(path).map_err(|_| missing())?;
    if meta.is_file() {
        if meta.len() as usize > MAX_BYTES {
            return Err(Refusal::new(format!("{shown} is over 512 MB"), "Shorten the clip or lower its bit rate."));
        }
        let data = std::fs::read(path).map_err(|e| Refusal::new(format!("{shown} could not be read: {e}"), "Check the mixer can read it."))?;
        let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(shown);
        return bytes(&name, data);
    }
    let mut files = Files::new();
    walk(path, path, &mut files).map_err(|e| Refusal::new(format!("{shown} could not be read: {e}"), "Check the mixer can read the folder."))?;
    if files.is_empty() {
        return Err(unknown(&shown));
    }
    from_tree(files, &shown)
}

fn walk(root: &Path, dir: &Path, out: &mut Files) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let meta = entry.metadata()?;
        let path = entry.path();
        let hidden = entry.file_name().to_string_lossy().starts_with('.');
        if meta.is_symlink() || hidden || out.len() > MAX_FILES {
            continue;
        }
        if meta.is_dir() {
            walk(root, &path, out)?;
        } else if let Some(rel) = path.strip_prefix(root).ok().and_then(|r| clean(&r.to_string_lossy())) {
            out.insert(rel, std::fs::read(&path)?);
        }
    }
    Ok(())
}
