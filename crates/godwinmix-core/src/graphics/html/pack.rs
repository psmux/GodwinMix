//! Where an HTML template comes from: the starter pack built into the
//! mixer, or an `.html` file in the media library.
//!
//! The pack is compiled in from `graphics/html/`, like the SVG pack, so every
//! install has it. The renderer loads a file, so the pack is written out
//! once to a folder of its own under the system's temporary folder, named by
//! a hash of its contents: an upgrade writes a new folder and never draws a
//! stale copy. A library template is loaded where it is, so a picture or a
//! font beside it in the library is found by its file name.

use super::{HtmlTemplate, TemplateOrigin};
use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};

macro_rules! pack {
    ($($name:literal),* $(,)?) => {
        &[$(($name, include_str!(concat!("../../../../../graphics/html/", $name, ".html")))),*]
    };
}

/// Every HTML template in the pack, by name, as written.
pub const PACK: &[(&str, &str)] = pack![
    "lower-third-glass",
    "lower-third-bold",
    "lower-third-line",
    "ticker-crawl",
    "ticker-flip",
    "score-bug-live",
    "logo-bug-shine",
    "countdown-ring",
    "title-card-3d",
    "logo-spin-3d",
    "starting-soon",
    "background-gradient",
    "background-particles",
    "set-newsroom",
    "set-studio",
];

/// The pack, read.
pub fn pack() -> Vec<HtmlTemplate> {
    PACK.iter().filter_map(|(name, html)| HtmlTemplate::parse(name, TemplateOrigin::Pack, html.to_string()).ok()).collect()
}

/// `name` as the library file it is, `.html` added when it has none.
pub fn library_name(name: &str) -> String {
    let name = name.trim().trim_start_matches("file://");
    if name.to_ascii_lowercase().ends_with(".html") {
        name.to_string()
    } else {
        format!("{name}.html")
    }
}

/// The template an `html:` address names, with the file the renderer loads.
pub fn load(name: &str) -> Result<HtmlTemplate> {
    let name = name.trim();
    if let Some((n, html)) = PACK.iter().find(|(n, _)| *n == name) {
        let mut t = HtmlTemplate::parse(n, TemplateOrigin::Pack, html.to_string())?;
        t.file = Some(page(n)?);
        return Ok(t);
    }
    let path = find(name)?;
    let html = std::fs::read_to_string(&path).with_context(|| format!("reading the HTML template {}", path.display()))?;
    let shown = if Path::new(name).is_absolute() { path.display().to_string() } else { library_name(name) };
    let mut t = HtmlTemplate::parse(&shown, TemplateOrigin::Library, html)?;
    t.file = Some(path);
    Ok(t)
}

fn find(name: &str) -> Result<PathBuf> {
    let file = library_name(name);
    let path = Path::new(&file);
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }
    let pack: Vec<&str> = PACK.iter().map(|(n, _)| *n).collect();
    let unknown = || format!("there is no HTML template {name:?}. The pack has {}, and the media library has whatever template.list shows", pack.join(", "));
    if file.split(['/', '\\']).any(|p| p == ".." || p.is_empty()) {
        bail!("{}", unknown());
    }
    let Some(dir) = crate::graphics::brand::library() else { bail!("{}", unknown()) };
    let path = dir.join(&file);
    if !path.is_file() {
        bail!("{}", unknown());
    }
    Ok(path)
}

/// The pack's copy of `name` on disk, written out the first time.
pub fn page(name: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!("godwinmix-html-{:016x}", hash()));
    let path = dir.join(format!("{name}.html"));
    if path.is_file() {
        return Ok(path);
    }
    let (_, html) = PACK.iter().find(|(n, _)| *n == name).with_context(|| format!("{name} is not in the pack"))?;
    std::fs::create_dir_all(&dir).with_context(|| format!("making {}", dir.display()))?;
    let part = dir.join(format!(".{name}.{}.part", std::process::id()));
    std::fs::write(&part, html).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, &path).with_context(|| format!("writing {}", path.display()))?;
    Ok(path)
}

/// FNV-1a over the whole pack: a new folder for every change to it.
fn hash() -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (name, html) in PACK {
        for b in name.bytes().chain(html.bytes()) {
            h = (h ^ b as u64).wrapping_mul(0x0100_0000_01b3);
        }
    }
    h
}

/// The library's HTML templates: every `.html` with a gmx-template block,
/// and the reason for each that would not read.
pub fn library(dir: &Path) -> (Vec<HtmlTemplate>, Vec<String>) {
    let (mut found, mut errors) = (Vec::new(), Vec::new());
    let Ok(entries) = std::fs::read_dir(dir) else { return (found, errors) };
    let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
    names.sort();
    for name in names.into_iter().filter(|n| n.to_ascii_lowercase().ends_with(".html") && !n.starts_with('.')) {
        let Ok(html) = std::fs::read_to_string(dir.join(&name)) else { continue };
        if !html.contains(super::meta::BLOCK_ID) {
            continue;
        }
        match HtmlTemplate::parse(&name, TemplateOrigin::Library, html) {
            Ok(mut t) => {
                t.file = Some(dir.join(&name));
                found.push(t);
            }
            Err(e) => errors.push(format!("{name}: {e}")),
        }
    }
    (found, errors)
}

/// Check `html` and write it into the library as `name`. Refuses to write
/// over a file unless `replace`, and never writes a template with an error.
pub fn save(dir: &Path, name: &str, html: &str, replace: bool) -> Result<HtmlTemplate> {
    let file = library_name(name);
    // The library's own rules for a name, which take only media extensions.
    crate::media::safe_upload_name(&format!("{}.svg", file.trim_end_matches(".html")))?;
    let mut t = HtmlTemplate::parse(&file, TemplateOrigin::Library, html.to_string())?;
    let path = dir.join(&file);
    if path.exists() && !replace {
        bail!("the media library already has {file}. Save it under another name, or pass replace: true to write over it");
    }
    std::fs::create_dir_all(dir).with_context(|| format!("making the media folder {}", dir.display()))?;
    let part = dir.join(format!(".{file}.part"));
    std::fs::write(&part, html).with_context(|| format!("writing {}", part.display()))?;
    std::fs::rename(&part, &path).with_context(|| format!("saving {}", path.display()))?;
    t.file = Some(path);
    Ok(t)
}
