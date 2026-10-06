//! The gallery on disk: list it, find one item, write one, take one away.
//!
//! A write goes into a hidden folder first and is renamed into place, so a
//! reader never sees half an item and a failed save leaves nothing behind.
//! Replacing an item swaps the old folder out before the new one goes in.

use super::draft::Draft;
use super::entry::{Entry, Where};
use super::manifest::Manifest;
use super::{is_slug, starters, MANIFEST, MARKER};
use crate::graphics::pack;
use anyhow::{bail, Context, Result};
use godwinmix_protocol::gallery::Origin;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Every item: saved folders, then the starters, the pack and the library's
/// SVG templates, each id once. The reasons any folder would not read come
/// back beside the list.
pub fn list(gallery: &Path) -> (Vec<Entry>, Vec<String>) {
    // The starter transitions and effects are folders like any saved item.
    let starters_fx = crate::fx::starter::install(gallery);
    let (mut out, mut errors) = saved(gallery);
    if let Err(e) = starters_fx {
        errors.push(format!("the starter transitions could not be written: {e:#}"));
    }
    let mut seen: HashSet<String> = out.iter().map(|e| e.item.id.clone()).collect();
    for s in starters::STARTERS {
        match s.manifest().and_then(|m| Ok((s.materialise(gallery)?, m))) {
            Ok((dir, m)) if seen.insert(s.id.to_string()) => out.push(Entry::folder(s.id, dir, m, Some(s))),
            Ok(_) => {}
            Err(e) => errors.push(format!("starter {}: {e:#}", s.id)),
        }
    }
    for t in pack::pack() {
        let name = pack::PACK.iter().map(|(n, _)| *n).find(|n| *n == t.info.name).unwrap_or("template");
        if seen.insert(name.to_string()) {
            out.push(templated(name, Where::Pack(name), &t, Origin::Shipped));
        }
    }
    if let Some(lib) = crate::graphics::brand::library() {
        let (found, bad) = pack::library(&lib);
        errors.extend(bad);
        for t in found {
            let id = super::slug(&t.info.name.trim_end_matches(".svg").replace('.', "-"));
            if seen.insert(id.clone()) {
                out.push(templated(&id, Where::Library(t.info.name.clone()), &t, Origin::Uploaded));
            }
        }
    }
    (out, errors)
}

/// A pack or library template, as an entry.
fn templated(id: &str, at: Where, t: &crate::graphics::Template, origin: Origin) -> Entry {
    let manifest = Manifest {
        name: t.info.title.clone(),
        kind: "template".into(),
        description: t.info.description.clone(),
        origin: Some(if origin == Origin::Shipped { "shipped" } else { "uploaded" }.into()),
        ..Default::default()
    };
    let mut item = super::entry::item(id, &manifest, None);
    item.fields = t.info.fields.clone();
    item.uri = Some(t.info.uri.clone());
    item.zone = pack_zone(&t.info.name).unwrap_or(item.zone);
    item.transparent = t.info.name != "title-card";
    Entry { item, manifest, at, starter: None }
}

/// What each pack template is for. Each is laid out on a whole frame, so
/// it is still placed over the whole canvas; the zone says what it is.
fn pack_zone(name: &str) -> Option<godwinmix_protocol::gallery::Zone> {
    use godwinmix_protocol::gallery::Zone;
    Some(match name {
        "news-lower-third" | "breaking-news" | "headline-strap" | "location-tag" => Zone::LowerThird,
        "score-bug" | "logo-bug" => Zone::Bug,
        "title-card" => Zone::Full,
        "quote-card" => Zone::Center,
        _ => return None,
    })
}

/// The folders in the gallery, newest first.
fn saved(gallery: &Path) -> (Vec<Entry>, Vec<String>) {
    let (mut out, mut errors) = (Vec::new(), Vec::new());
    let Ok(dirs) = std::fs::read_dir(gallery) else { return (out, errors) };
    for d in dirs.flatten() {
        let id = d.file_name().to_string_lossy().to_string();
        let path = d.path();
        if !is_slug(&id) || !path.join(MANIFEST).is_file() {
            continue;
        }
        match Manifest::read(&path) {
            Ok(m) => out.push(Entry::folder(&id, path, m, None)),
            Err(e) => errors.push(format!("{id}: {e:#}")),
        }
    }
    out.sort_by(|a, b| b.item.saved.cmp(&a.item.saved).then_with(|| a.item.name.cmp(&b.item.name)));
    (out, errors)
}

/// One item, or the error that names the ones there are.
pub fn find(gallery: &Path, id: &str) -> Result<Entry> {
    let id = id.trim();
    let (all, _) = list(gallery);
    let wanted = super::slug(id);
    if let Some(e) = all.iter().find(|e| e.item.id == id || e.item.id == wanted || e.item.name.eq_ignore_ascii_case(id)) {
        return Ok(e.clone());
    }
    let ids: Vec<&str> = all.iter().map(|e| e.item.id.as_str()).take(30).collect();
    bail!("the gallery has no item {id:?}. It has {}. Call gallery.list to search it", ids.join(", "))
}

/// Write `draft` as item `id`.
pub fn write(gallery: &Path, id: &str, draft: &Draft, replace: bool) -> Result<PathBuf> {
    anyhow::ensure!(is_slug(id), "{id:?} is not an id: use lower case letters, digits and dashes");
    if starters::find(id).is_some() || pack::PACK.iter().any(|(n, _)| *n == id) {
        bail!("{id} is a design that ships with the mixer and cannot be written over. Save it under another name");
    }
    prepare(gallery)?;
    let dest = gallery.join(id);
    if dest.exists() && !replace {
        bail!("the gallery already has {id}. Save again with replace: true to write over it, or give another name");
    }
    let part = gallery.join(format!(".{id}.part-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&part);
    std::fs::create_dir_all(&part).with_context(|| format!("making {}", part.display()))?;
    for (name, bytes) in &draft.files {
        let path = part.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, bytes).with_context(|| format!("writing {}", path.display()))?;
    }
    std::fs::write(part.join(MANIFEST), draft.manifest.to_toml()?)?;
    swap_in(gallery, id, &part, &dest)?;
    Ok(dest)
}

fn swap_in(gallery: &Path, id: &str, part: &Path, dest: &Path) -> Result<()> {
    let old = gallery.join(format!(".{id}.old-{}", std::process::id()));
    if dest.exists() {
        std::fs::rename(dest, &old).with_context(|| format!("moving the old {id} aside; is a file in it open?"))?;
    }
    if let Err(e) = std::fs::rename(part, dest) {
        let _ = std::fs::rename(&old, dest);
        return Err(e).with_context(|| format!("putting {id} in place"));
    }
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

/// Make the gallery's folder and its marker.
pub fn prepare(gallery: &Path) -> Result<()> {
    std::fs::create_dir_all(gallery).with_context(|| format!("making the gallery folder {}", gallery.display()))?;
    let marker = gallery.join(MARKER);
    if !marker.is_file() {
        std::fs::write(&marker, "The GodwinMix Graphics gallery. Each folder here is one item; see docs/reference/gallery-format.md.\n")?;
    }
    Ok(())
}

/// Take a saved item away, with its pictures.
pub fn remove(gallery: &Path, id: &str) -> Result<PathBuf> {
    let entry = find(gallery, id)?;
    match (&entry.at, entry.read_only()) {
        (_, true) => bail!("{} ships with the mixer and cannot be deleted", entry.item.id),
        (Where::Folder(dir), false) => {
            std::fs::remove_dir_all(dir).with_context(|| format!("deleting {}", dir.display()))?;
            super::preview::forget(gallery, &entry.item.id);
            Ok(dir.clone())
        }
        (Where::Library(_), false) => {
            let file = entry.file().context("the library file")?;
            std::fs::remove_file(&file).with_context(|| format!("deleting {}", file.display()))?;
            Ok(file)
        }
        (Where::Pack(_), false) => bail!("{} ships with the mixer and cannot be deleted", entry.item.id),
    }
}

/// An id not yet taken, made from `name`.
pub fn free_id(gallery: &Path, name: &str) -> String {
    let base = super::slug(name);
    let (all, _) = list(gallery);
    let taken: HashSet<&str> = all.iter().map(|e| e.item.id.as_str()).collect();
    (1..).map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") }).find(|id| !taken.contains(id.as_str())).unwrap_or(base)
}
