//! Moving items between mixers: one zip out, and anything in.
//!
//! An export is a stored zip with a folder per item and a `gallery.json`
//! naming them (`docs/reference/gallery-format.md`). An import takes that,
//! and also a folder or zip of several items, a folder of loose files, or a
//! lone file, and answers a draft or a refusal for each thing in it, so one
//! bad file does not stop the rest.

use super::draft::{self, Draft, Files, Refusal};
use super::edit::as_draft;
use super::entry::Entry;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::Path;

/// Write `entries` as one zip at `dest`, and answer its size.
pub fn export(entries: &[Entry], dest: &Path) -> Result<u64> {
    let mut zip = crate::zip::Zip::new();
    let mut ids = Vec::new();
    for e in entries {
        let d = as_draft(e).with_context(|| format!("reading {}", e.item.id))?;
        let mut m = d.manifest.clone();
        m.origin = Some(if e.read_only() { "uploaded".into() } else { m.origin.unwrap_or_else(|| "agent".into()) });
        anyhow::ensure!(zip.add(&format!("{}/{}", e.item.id, super::MANIFEST), m.to_toml()?.as_bytes()), "{} is too large for a zip", e.item.id);
        for (name, bytes) in &d.files {
            anyhow::ensure!(zip.add(&format!("{}/{name}", e.item.id), bytes), "{}/{name} is over 4 GB", e.item.id);
        }
        ids.push(e.item.id.clone());
    }
    let index = serde_json::json!({"format": "godwinmix-gallery", "version": 1, "items": ids});
    zip.add("gallery.json", serde_json::to_string_pretty(&index)?.as_bytes());
    let bytes = zip.finish();
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("making {}", parent.display()))?;
    }
    let part = dest.with_extension("zip.part");
    std::fs::write(&part, &bytes).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, dest).with_context(|| format!("saving {}", dest.display()))?;
    Ok(bytes.len() as u64)
}

/// Every item in a file, folder or zip: a name for each and its draft or
/// the reason it was refused.
pub fn read_path(path: &Path) -> Vec<(String, Result<Draft, Refusal>)> {
    let shown = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| path.display().to_string());
    if path.is_dir() {
        let mut files = Files::new();
        if let Err(e) = collect(path, path, &mut files) {
            return vec![(shown, Err(Refusal::new(format!("the folder could not be read: {e}"), "Check the mixer can read it.")))];
        }
        return split(files, &shown);
    }
    match std::fs::read(path) {
        Ok(bytes) => read_bytes(&shown, bytes),
        Err(_) => vec![(shown.clone(), draft::from_path(path))],
    }
}

/// The same for bytes that arrived with a name.
pub fn read_bytes(name: &str, bytes: Vec<u8>) -> Vec<(String, Result<Draft, Refusal>)> {
    if super::detect::sniff(&bytes, name) == Some(super::detect::Sniffed::Zip) {
        return match crate::zip::read(&bytes) {
            Ok(members) => split(members.into_iter().collect(), name),
            Err(e) => vec![(name.to_string(), Err(Refusal::new(format!("{name}: {e}"), "Zip it again with no compression (zip -0, or export it from a GodwinMix gallery), or import the folder itself.")))],
        };
    }
    vec![(name.to_string(), draft::bytes(name, bytes))]
}

/// Cut a tree into items: one when its root is one, else one per folder
/// that is one and one per loose file.
fn split(files: Files, name: &str) -> Vec<(String, Result<Draft, Refusal>)> {
    let files: Files = files.into_iter().filter(|(k, _)| k != "gallery.json" && !k.split('/').any(|p| p.starts_with('.'))).collect();
    let is_item = |f: &Files| f.contains_key(super::MANIFEST) || f.contains_key("index.html") || f.keys().any(|k| k.ends_with(".ograf.json") && !k.contains('/'));
    let mut groups: BTreeMap<String, Files> = BTreeMap::new();
    for (k, v) in &files {
        let (top, rest) = k.split_once('/').unwrap_or(("", k));
        groups.entry(top.to_string()).or_default().insert(rest.to_string(), v.clone());
    }
    if is_item(&files) || (groups.len() == 1 && !groups.contains_key("") && groups.values().all(is_item)) {
        return vec![(name.to_string(), draft::from_tree(files, name))];
    }
    let mut out = Vec::new();
    for (top, group) in groups {
        if top.is_empty() {
            out.extend(group.into_iter().map(|(file, bytes)| (file.clone(), draft::bytes(&file, bytes))));
        } else if is_item(&group) {
            out.push((top.clone(), draft::from_tree(group, &top)));
        } else {
            out.extend(split(group, &top));
        }
    }
    out
}

fn collect(root: &Path, dir: &Path, out: &mut Files) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)?.flatten() {
        let (path, meta) = (entry.path(), entry.metadata()?);
        if meta.is_symlink() || entry.file_name().to_string_lossy().starts_with('.') || out.len() > 2000 {
            continue;
        }
        if meta.is_dir() {
            collect(root, &path, out)?;
        } else if let Some(rel) = path.strip_prefix(root).ok().and_then(|r| draft::tree_clean(&r.to_string_lossy())) {
            out.insert(rel, std::fs::read(&path)?);
        }
    }
    Ok(())
}
