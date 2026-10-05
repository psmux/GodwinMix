//! Where a template comes from: the starter pack built into the mixer, or
//! an SVG in the media library.
//!
//! The pack is compiled in from `graphics/` at the root of the repository,
//! so every install has it with nothing to copy and an upgrade brings the
//! new versions. It is read only on purpose: a station that wants to change
//! one copies it into its library (`template.get`, then `template.save`),
//! and its copy is never overwritten by an upgrade.

use super::template::{Template, TemplateOrigin};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

macro_rules! pack {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../../../../graphics/", $name, ".svg")))),*]
    };
}

/// Every template in the pack, by name, as written.
pub const PACK: &[(&str, &str)] = pack![
    "news-lower-third",
    "breaking-news",
    "headline-strap",
    "score-bug",
    "logo-bug",
    "title-card",
    "quote-card",
    "location-tag",
    "set-newsroom-desk",
    "set-studio-frame",
];

/// The pack, read.
pub fn pack() -> Vec<Template> {
    PACK.iter().filter_map(|(name, svg)| Template::parse(name, TemplateOrigin::Pack, svg.to_string()).ok()).collect()
}

/// The template a `template:` address or a `template.get` names: a pack
/// name, a file in the media library (with or without `.svg`), or an
/// absolute path.
pub fn load(name: &str) -> Result<Template> {
    let name = name.trim();
    if let Some((_, svg)) = PACK.iter().find(|(n, _)| *n == name) {
        return Template::parse(name, TemplateOrigin::Pack, svg.to_string());
    }
    let path = find(name)?;
    let svg = std::fs::read_to_string(&path).with_context(|| format!("reading the template {}", path.display()))?;
    let shown = if Path::new(name).is_absolute() { path.display().to_string() } else { library_name(name) };
    Template::parse(&shown, TemplateOrigin::Library, svg)
}

/// `name` as the library file it is, `.svg` added when it has none.
pub fn library_name(name: &str) -> String {
    let name = name.trim().trim_start_matches("file://");
    if name.to_ascii_lowercase().ends_with(".svg") {
        name.to_string()
    } else {
        format!("{name}.svg")
    }
}

fn find(name: &str) -> Result<PathBuf> {
    let file = library_name(name);
    let path = Path::new(&file);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let pack: Vec<&str> = PACK.iter().map(|(n, _)| *n).collect();
    let unknown = || format!("there is no template {name:?}. The pack has {}, and the media library has whatever template.list shows", pack.join(", "));
    if file.split(['/', '\\']).any(|p| p == ".." || p.is_empty()) {
        bail!("{}", unknown());
    }
    let Some(dir) = super::brand::library() else { bail!("{}", unknown()) };
    let path = dir.join(&file);
    if !path.is_file() {
        bail!("{}", unknown());
    }
    Ok(path)
}

/// The library's templates: every SVG with a field marker or a
/// `<gmx:template>` in it, and the reason for each that would not read.
pub fn library(dir: &Path) -> (Vec<Template>, Vec<String>) {
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else { return (found, errors) };
    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    for name in names.into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".svg") && !n.starts_with('.')) {
        let Ok(svg) = std::fs::read_to_string(dir.join(&name)) else { continue };
        if !svg.contains("{{") && !svg.contains("<gmx:template") {
            continue;
        }
        match Template::parse(&name, TemplateOrigin::Library, svg) {
            Ok(t) => found.push(t),
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    (found, errors)
}

/// Check `svg` and write it into the library as `name`. Refuses to write
/// over a file unless `replace`, and never writes a template that would
/// not render.
pub fn save(dir: &Path, name: &str, svg: &str, replace: bool) -> Result<Template> {
    let file = crate::media::safe_upload_name(&library_name(name))?;
    let template = Template::parse(&file, TemplateOrigin::Library, svg.to_string())?;
    let path = dir.join(&file);
    if path.exists() && !replace {
        bail!("the media library already has {file}. Save it under another name, or pass replace: true to write over it");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("making the media folder {}", dir.display()))?;
    let part = dir.join(format!(".{file}.part"));
    std::fs::write(&part, svg).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, &path).with_context(|| format!("saving {}", path.display()))?;
    Ok(template)
}
