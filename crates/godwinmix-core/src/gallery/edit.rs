//! Changing an item that is already in the gallery: what is said about it,
//! and copies of it.

use super::draft::{self, Draft};
use super::entry::{Entry, Where};
use super::manifest::Manifest;
use super::store;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Change a saved item's manifest in place.
pub fn edit(gallery: &Path, id: &str, change: impl FnOnce(&mut Manifest)) -> Result<Entry> {
    let entry = store::find(gallery, id)?;
    let Some(dir) = entry.dir().filter(|_| !entry.read_only()).map(Path::to_path_buf) else {
        bail!("{} ships with the mixer and cannot be changed. Duplicate it with gallery.duplicate and change the copy", entry.item.id);
    };
    let mut m = Manifest::read(&dir)?;
    change(&mut m);
    let part = dir.join(format!(".{}.part", super::MANIFEST));
    std::fs::write(&part, m.to_toml()?).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, dir.join(super::MANIFEST)).context("saving graphic.toml")?;
    store::find(gallery, &entry.item.id)
}

/// A copy of any item, shipped ones included, under a new id.
pub fn duplicate(gallery: &Path, id: &str, name: Option<&str>) -> Result<Entry> {
    let entry = store::find(gallery, id)?;
    let mut d = as_draft(&entry)?;
    let name = name.map(str::trim).filter(|n| !n.is_empty()).map(str::to_string).unwrap_or_else(|| format!("{} copy", entry.item.name));
    d.manifest.name = name.clone();
    d.manifest.origin = Some(if entry.read_only() { "uploaded".into() } else { d.manifest.origin.clone().unwrap_or_else(|| "agent".into()) });
    d.manifest.saved = now();
    let new_id = store::free_id(gallery, &name);
    store::write(gallery, &new_id, &d, false)?;
    store::find(gallery, &new_id)
}

/// An item's files and manifest as a draft, ready to write elsewhere.
pub fn as_draft(entry: &Entry) -> Result<Draft> {
    match &entry.at {
        Where::Folder(dir) => draft::from_path(dir).map_err(|r| anyhow::anyhow!("{r}")),
        Where::Pack(name) => {
            let t = crate::graphics::pack::load(name)?;
            templated(entry, &t.svg)
        }
        Where::Library(_) => {
            let file = entry.file().context("the library file")?;
            templated(entry, &std::fs::read_to_string(&file).with_context(|| format!("reading {}", file.display()))?)
        }
    }
}

fn templated(entry: &Entry, svg: &str) -> Result<Draft> {
    let mut d = draft::svg(svg).map_err(|r| anyhow::anyhow!("{r}"))?;
    d.manifest.name = entry.item.name.clone();
    d.manifest.description = entry.item.description.clone();
    d.manifest.zone = Some(entry.item.zone.as_str().into());
    Ok(d)
}

/// Now, as RFC 3339.
pub fn now() -> String {
    crate::observe::logs::rfc3339(&std::time::SystemTime::now())
}
