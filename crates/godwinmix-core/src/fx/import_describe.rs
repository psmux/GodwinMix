//! The manifest a measured file gets, and the pieces of a pack that are not
//! media: its licence, its folders that are items already, its zip.

use super::super::{detect, library};
use anyhow::{bail, Context, Result};
use godwinmix_protocol::fx::{FxBlend, FxEntry, FxImportRequest, FxKind, FxManifest};
use std::path::{Path, PathBuf};

/// The manifest for one file, and its measurement when it was decoded.
pub fn describe(path: &Path, slug: &str, file_name: &str, req: &FxImportRequest) -> Result<(FxManifest, Option<detect::Measured>)> {
    let title = path.file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_default();
    let mut m = FxManifest {
        name: slug.to_string(),
        title: title.trim().to_string(),
        kind: FxKind::Shader,
        blend: FxBlend::Normal,
        file: file_name.to_string(),
        duration_ms: 1000,
        cut_at_ms: req.cut_at_ms,
        cut_at_measured_ms: None,
        coverage: None,
        softness: None,
        invert: false,
        transition: true,
        effect: false,
        licence: None,
        source: Some(path.file_name().unwrap_or_default().to_string_lossy().to_string()),
    };
    if ["glsl", "frag", "fs"].contains(&super::ext(path).as_str()) {
        let source = std::fs::read_to_string(path).context("reading the shader")?;
        super::super::shader::constants(&source)?;
        return Ok((m, None));
    }
    let measured = detect::measure(path)?;
    let v = detect::classify(&measured);
    m.kind = req.kind.unwrap_or(v.kind);
    m.blend = req.blend.unwrap_or(v.blend);
    m.cut_at_measured_ms = v.cut_at_ms;
    m.coverage = v.coverage;
    m.transition = v.transition || req.kind.is_some();
    m.effect = v.effect && matches!(m.kind, FxKind::Stinger | FxKind::Overlay);
    match m.kind {
        FxKind::Matte => m.softness = Some(0.1),
        FxKind::Shader => bail!("this is a picture or a clip, not a shader. Import it without kind, or as stinger, overlay or matte"),
        _ if measured.still() => bail!("a single picture cannot be a {:?}: it does not move. Import it as a matte, or add it as a source", m.kind),
        _ => m.duration_ms = measured.duration_ms.clamp(1, crate::mixer::transition::MAX_DURATION_MS),
    }
    Ok((m, Some(measured)))
}

/// A folder that already has an `fx.json`, copied in as it is.
pub fn copy_item(dir: &Path, root: &Path, replace: bool) -> Result<FxEntry> {
    let m = library::read(dir)?;
    let mut slug = library::slug(&m.name);
    if godwinmix_protocol::requests::TRANSITIONS.contains(&slug.as_str()) {
        slug.push_str("-fx");
    }
    let target = root.join(&slug);
    if target.exists() && !replace {
        bail!("the library already has {slug}. Pass replace: true to write over it");
    }
    let _ = std::fs::remove_dir_all(&target);
    std::fs::create_dir_all(&target)?;
    for entry in std::fs::read_dir(dir)?.flatten().filter(|e| e.path().is_file()) {
        std::fs::copy(entry.path(), target.join(entry.file_name()))?;
    }
    let m = FxManifest { name: slug, ..m };
    library::save(&target, &m)?;
    Ok(library::entry(&m, &target))
}

/// A licence or read me in `dir`, and its first line that says anything.
pub fn licence_in(dir: &Path) -> Option<(PathBuf, String)> {
    let entries = std::fs::read_dir(dir).ok()?;
    let mut names: Vec<PathBuf> = entries.flatten().map(|e| e.path()).filter(|p| p.is_file()).collect();
    names.sort();
    let file = names.into_iter().find(|p| {
        let n = p.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        ["licence", "license", "copying", "readme", "terms"].iter().any(|w| n.starts_with(w))
    })?;
    let text = std::fs::read_to_string(&file).ok()?;
    let first = text.lines().map(str::trim).find(|l| !l.is_empty())?.chars().take(200).collect();
    Some((file, first))
}

/// The licence in the folder a file sits in.
pub fn licence_beside(path: &Path) -> Option<(PathBuf, String)> {
    licence_in(path.parent()?)
}

/// Unpack a zip into a folder of its own under the library, for the import
/// to walk. Paths inside it that climb out are refused.
pub fn unzip(zip: &Path, root: &Path) -> Result<PathBuf> {
    let bytes = std::fs::read(zip).with_context(|| format!("reading {}", zip.display()))?;
    let members = crate::zip::read(&bytes).map_err(|e| anyhow::anyhow!("{e}"))?;
    let out = root.join(format!(".import-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)));
    for (name, data) in members {
        let rel = Path::new(&name);
        if rel.is_absolute() || rel.components().any(|c| !matches!(c, std::path::Component::Normal(_))) {
            continue;
        }
        let at = out.join(rel);
        if let Some(parent) = at.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&at, data).with_context(|| format!("unpacking {name}"))?;
    }
    Ok(out)
}
