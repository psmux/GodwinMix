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

pub use super::library::{library, save};
