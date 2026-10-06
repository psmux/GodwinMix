//! An item before it is written: its manifest and its files, made from
//! whatever a caller had. An SVG, a page, a picture's bytes, a folder or a
//! zip each become one, and each is checked here, so nothing reaches the
//! gallery's folder that the gallery cannot draw.
//!
//! A refusal is two sentences, what was wrong and what to do, because the
//! import dialog shows them in two places and a model acts on the second.

#[path = "draft_tree.rs"]
mod tree;

use super::detect::{sniff, Sniffed};
use super::manifest::{Manifest, SourceSpec};
use godwinmix_protocol::gallery::GalleryKind;
use std::path::Path;

pub use tree::{clean as tree_clean, from_path, from_tree, Files};

/// The largest item, all its files together.
pub const MAX_BYTES: usize = 512 * 1024 * 1024;

#[derive(Debug, Clone, Default)]
pub struct Draft {
    pub manifest: Manifest,
    /// Relative paths with `/`, and their bytes.
    pub files: Vec<(String, Vec<u8>)>,
    /// What does not stop the save and is worth fixing.
    pub warnings: Vec<String>,
}

/// Why something was not taken, and what to do about it.
#[derive(Debug, Clone, PartialEq)]
pub struct Refusal {
    pub reason: String,
    pub fix: String,
}

impl Refusal {
    pub fn new(reason: impl Into<String>, fix: impl Into<String>) -> Refusal {
        Refusal { reason: reason.into(), fix: fix.into() }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}. {}", self.reason.trim_end_matches('.'), self.fix)
    }
}

impl std::error::Error for Refusal {}

fn manifest(kind: GalleryKind, file: &str) -> Manifest {
    Manifest { kind: kind.as_str().into(), file: file.into(), ..Default::default() }
}

/// An SVG: a template when it has `{{fields}}` or a `<gmx:template>`, a
/// picture when it has neither.
pub fn svg(svg: &str) -> Result<Draft, Refusal> {
    if !svg.contains("<svg") {
        return Err(Refusal::new("this is not an SVG: there is no <svg> element in it", "Send the whole document, from <svg to </svg>."));
    }
    let templated = svg.contains("{{") || svg.contains("<gmx:template");
    if templated {
        crate::graphics::Template::parse("graphic.svg", crate::graphics::template::TemplateOrigin::Library, svg.to_string())
            .map_err(|e| Refusal::new(format!("the SVG template does not read: {e:#}"), "Fix that and save again; get_template shows a pack template that works."))?;
    } else if crate::plugin::kinds::image_probe::svg_size(svg).is_none() {
        return Err(Refusal::new("the SVG says no size", "Give the <svg> a viewBox, such as viewBox=\"0 0 1920 1080\", and width and height."));
    }
    let kind = if templated { GalleryKind::Template } else { GalleryKind::Image };
    let mut d = Draft { manifest: manifest(kind, "graphic.svg"), files: vec![("graphic.svg".into(), svg.as_bytes().to_vec())], ..Default::default() };
    // A namespace is an address in name only; a picture or a font loaded
    // from one is the thing that would be missing offline.
    if ["href=\"http", "href='http", "url(http", "url('http", "url(\"http"].iter().any(|p| svg.contains(p)) {
        d.warnings.push("the SVG names an address on the network; pictures in a template come from data: URIs or the media library".into());
    }
    Ok(d)
}

/// A web page, with anything it loads beside it.
pub fn html(page: &str, extra: Vec<(String, Vec<u8>)>) -> Result<Draft, Refusal> {
    let lower = page.to_ascii_lowercase();
    if !lower.contains("<html") && !lower.contains("<body") && !lower.contains("<!doctype") && !lower.contains("<div") && !lower.contains("<svg") {
        return Err(Refusal::new("this does not look like an HTML page", "Send a whole page, from <!doctype html> to </html>, with its CSS and script inline."));
    }
    let mut d = Draft { manifest: manifest(GalleryKind::Html, "index.html"), ..Default::default() };
    d.files.push(("index.html".into(), page.as_bytes().to_vec()));
    for (name, bytes) in extra {
        d.files.push((tree::clean(&name).ok_or_else(|| bad_name(&name))?, bytes));
    }
    if lower.contains("src=\"http") || lower.contains("href=\"http") || lower.contains("url(http") {
        d.warnings.push("the page loads something from the network; it will be missing whenever the mixer is offline. Put fonts and pictures beside the page in `files`".into());
    }
    if !lower.contains("transparent") && !lower.contains("background") {
        d.warnings.push("the page sets no background, which the browser source draws transparent; that is right for a graphic over a camera".into());
    }
    Ok(d)
}

/// A ticker or a text, kept as the source it adds.
pub fn source(uri: &str, params: toml::Table) -> Result<Draft, Refusal> {
    let lower = uri.trim().to_ascii_lowercase();
    let kind = if lower.starts_with("ticker:") {
        GalleryKind::Ticker
    } else if lower.starts_with("text:") {
        GalleryKind::Text
    } else {
        return Err(Refusal::new(format!("{uri:?} is not a ticker: or text: address"), "Give source as {\"uri\": \"ticker:\", \"params\": {\"items\": [\"...\"]}}, or use svg, html or data for anything else."));
    };
    let mut m = manifest(kind, "");
    m.source = Some(SourceSpec { uri: uri.trim().into(), params });
    Ok(Draft { manifest: m, ..Default::default() })
}

/// The bytes of one file, whatever it is.
pub fn bytes(name: &str, bytes: Vec<u8>) -> Result<Draft, Refusal> {
    // A shader, or a light leak on black, is a transition or an effect.
    if let Some(fx) = crate::fx::gallery_import::draft(name, &bytes) {
        return fx;
    }
    match sniff(&bytes, name) {
        Some(Sniffed::Svg) => svg(&String::from_utf8_lossy(&bytes)),
        Some(Sniffed::Html) => html(&String::from_utf8_lossy(&bytes), Vec::new()),
        Some(Sniffed::Zip) => {
            let members = crate::zip::read(&bytes).map_err(|e| Refusal::new(format!("{name}: {e}"), "Zip it again with no compression (zip -0, or export it from a GodwinMix gallery), or import the folder itself."))?;
            from_tree(members.into_iter().collect(), name)
        }
        Some(s @ (Sniffed::Png | Sniffed::Jpeg | Sniffed::Webp | Sniffed::Gif | Sniffed::Webm | Sniffed::Mov)) => {
            let kind = s.kind().unwrap_or(GalleryKind::Image);
            let file = format!("{}.{}", if kind == GalleryKind::Clip { "clip" } else { "picture" }, s.ext());
            Ok(Draft { manifest: manifest(kind, &file), files: vec![(file, bytes)], ..Default::default() })
        }
        _ => Err(unknown(name)),
    }
}

pub(crate) fn unknown(name: &str) -> Refusal {
    Refusal::new(
        format!("{name} is not a kind of file the gallery takes"),
        "Give an SVG, an HTML page, a PNG, WebP or JPEG picture, a WebM or MOV clip, an OGraf package, or a zip of one of these.",
    )
}

pub(crate) fn bad_name(name: &str) -> Refusal {
    Refusal::new(format!("{name:?} is not a file name the gallery writes"), "Use a plain relative name such as fonts/brand.woff2, with no .. and no drive letter.")
}

/// The name a file had, for a draft that has no better one.
pub fn stem(name: &str) -> String {
    Path::new(name.trim_end_matches(['/', '\\'])).file_stem().map(|s| s.to_string_lossy().replace(['_', '-'], " ")).unwrap_or_else(|| "graphic".into())
}
