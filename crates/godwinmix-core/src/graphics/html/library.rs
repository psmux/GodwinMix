//! HTML templates in the media library: listing them and saving one.

use super::pack::library_name;
use super::{HtmlTemplate, TemplateOrigin};
use anyhow::{bail, Context, Result};
use std::path::Path;

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
