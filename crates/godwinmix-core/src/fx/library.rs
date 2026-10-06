//! The library: the gallery's folder, `graphics/` in the media folder, one
//! folder an item, each with a `graphic.toml` beside its file. Only the
//! items whose `kind` is `transition` or `effect` are ours; see `toml_form`.

use super::starter;
use anyhow::{bail, Context, Result};
use godwinmix_protocol::fx::{FxEntry, FxManifest};
use std::path::{Path, PathBuf};

/// The manifest's file name inside an item's folder.
pub const MANIFEST: &str = "graphic.toml";

/// Every item, sorted by name, and a sentence for each folder that would not
/// read. Writes the starter set first where it is missing.
/// `root` is the gallery's folder (`gallery::dir()`), so the gallery lists
/// every transition and effect with its other designs.
pub fn list(root: &Path) -> (Vec<(FxManifest, PathBuf)>, Vec<String>) {
    let mut errors = Vec::new();
    if let Err(e) = starter::install(root) {
        errors.push(format!("the starter set could not be written: {e:#}"));
    }
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else { return (found, errors) };
    for dir in entries.flatten().map(|e| e.path()).filter(|p| p.is_dir()) {
        match read(&dir) {
            Ok(Some(m)) => found.push((m, dir)),
            Ok(None) => {}
            Err(e) if dir.join(MANIFEST).exists() => errors.push(format!("{}: {e:#}", dir.display())),
            Err(_) => {}
        }
    }
    found.sort_by(|a, b| a.0.name.cmp(&b.0.name));
    (found, errors)
}

/// The item in one folder, checked against what is there. `None` for a
/// gallery item that is not a transition or an effect.
pub fn read(dir: &Path) -> Result<Option<FxManifest>> {
    let text = std::fs::read_to_string(dir.join(MANIFEST)).with_context(|| format!("reading {}", dir.join(MANIFEST).display()))?;
    let id = dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let Some(m) = super::toml_form::parse(&id, &text)? else { return Ok(None) };
    if m.file.contains(['/', '\\']) || m.file.contains("..") || !dir.join(&m.file).is_file() {
        bail!("graphic.toml names the file {:?}, which is not in its folder", m.file);
    }
    Ok(Some(m))
}

/// The item called `name`, or an error that lists the names there are.
pub fn find(root: &Path, name: &str) -> Result<(FxManifest, PathBuf)> {
    let (all, _) = list(root);
    let wanted = slug(name);
    if let Some(hit) = all.iter().find(|(m, _)| m.name == wanted) {
        return Ok(hit.clone());
    }
    let names: Vec<&str> = all.iter().map(|(m, _)| m.name.as_str()).collect();
    bail!("there is no transition or effect called {name:?}. The library has: {}. Import one with fx.import", names.join(", "))
}

/// Write `m` into `dir`, through a temporary file so a reader never sees half.
/// What it keeps of the folder's last `graphic.toml` is everything that is
/// not an fx setting: the gallery's description, tags and the rest.
pub fn save(dir: &Path, m: &FxManifest) -> Result<()> {
    let old = std::fs::read_to_string(dir.join(MANIFEST)).ok();
    let text = super::toml_form::render(m, old.as_deref(), "uploaded")?;
    let part = dir.join(".graphic.toml.part");
    std::fs::write(&part, text).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, dir.join(MANIFEST)).context("saving graphic.toml")?;
    Ok(())
}

/// Take an item out of the library. A starter item is refused: it would be
/// written back the next time the library is read.
pub fn remove(root: &Path, name: &str) -> Result<()> {
    let (m, dir) = find(root, name)?;
    if starter::is_starter(&m.name) {
        bail!("{} ships with the mixer and comes back when the library is read. Turn it off instead with fx.set {{\"transition\": false, \"effect\": false}}", m.name);
    }
    std::fs::remove_dir_all(&dir).with_context(|| format!("removing {}", dir.display()))
}

/// What `fx.list` shows for one item.
pub fn entry(m: &FxManifest, dir: &Path) -> FxEntry {
    let runs = (m.kind == godwinmix_protocol::fx::FxKind::Shader).then(|| super::shader::runs(&m.name).to_string());
    let note = match runs.as_deref() {
        Some("fade") => Some("no GPU shaders on this machine and no software version of this one, so it runs as a dissolve".to_string()),
        _ => m.coverage.filter(|c| *c < 0.9 && m.transition && m.kind != godwinmix_protocol::fx::FxKind::Matte).map(|c| {
            format!("it covers {:.0} percent of the picture at its fullest, so the cut underneath may show", c * 100.0)
        }),
    };
    FxEntry {
        manifest: m.clone(),
        origin: if starter::is_starter(&m.name) { "starter" } else { "library" }.to_string(),
        dir: dir.display().to_string(),
        runs,
        preview: format!("/api/v1/fx/{}/preview.jpg", m.name),
        note,
    }
}

/// A name as a slug: lower case letters, digits and single hyphens.
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().chars().flat_map(|c| c.to_lowercase()) {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    let out = out.trim_end_matches('-').to_string();
    if out.is_empty() { "fx".to_string() } else { out }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_file_name_becomes_a_slug() {
        assert_eq!(super::slug("Light Leak 04 (warm).MOV"), "light-leak-04-warm-mov");
        assert_eq!(super::slug("  __ "), "fx");
    }
}
