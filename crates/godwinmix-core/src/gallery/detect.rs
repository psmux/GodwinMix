//! What a file is, read from its first bytes rather than trusted from its
//! name, and the guesses the gallery makes when an item does not say:
//! whether it moves, whether it is transparent, where it goes.

use godwinmix_protocol::gallery::{GalleryKind, Zone};
use std::path::Path;

/// What some bytes are.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sniffed {
    Svg,
    Html,
    Png,
    Jpeg,
    Webp,
    Gif,
    /// Matroska or WebM.
    Webm,
    /// QuickTime or MP4: an `ftyp` box.
    Mov,
    Zip,
    Toml,
    Json,
}

impl Sniffed {
    /// The extension a file of this kind is written with.
    pub fn ext(self) -> &'static str {
        match self {
            Self::Svg => "svg",
            Self::Html => "html",
            Self::Png => "png",
            Self::Jpeg => "jpg",
            Self::Webp => "webp",
            Self::Gif => "gif",
            Self::Webm => "webm",
            Self::Mov => "mov",
            Self::Zip => "zip",
            Self::Toml => "toml",
            Self::Json => "json",
        }
    }

    /// The gallery kind a lone file of this sort is.
    pub fn kind(self) -> Option<GalleryKind> {
        match self {
            Self::Png | Self::Jpeg | Self::Webp | Self::Gif => Some(GalleryKind::Image),
            Self::Webm | Self::Mov => Some(GalleryKind::Clip),
            Self::Html => Some(GalleryKind::Html),
            _ => None,
        }
    }
}

/// Look at the bytes, then the name for the text formats that have no magic.
pub fn sniff(bytes: &[u8], name: &str) -> Option<Sniffed> {
    let b = bytes;
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(Sniffed::Png);
    }
    if b.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(Sniffed::Jpeg);
    }
    if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return Some(Sniffed::Webp);
    }
    if b.starts_with(b"GIF8") {
        return Some(Sniffed::Gif);
    }
    if b.starts_with(&[0x1a, 0x45, 0xdf, 0xa3]) {
        return Some(Sniffed::Webm);
    }
    if b.len() > 12 && (&b[4..8] == b"ftyp" || &b[4..8] == b"moov" || &b[4..8] == b"wide" || &b[4..8] == b"mdat") {
        return Some(Sniffed::Mov);
    }
    if b.starts_with(b"PK\x03\x04") || b.starts_with(b"PK\x05\x06") {
        return Some(Sniffed::Zip);
    }
    let head = String::from_utf8_lossy(&b[..b.len().min(4096)]).to_ascii_lowercase();
    let head = head.trim_start_matches('\u{feff}').trim_start();
    let lower = name.to_ascii_lowercase();
    if head.starts_with("<!doctype html") || head.starts_with("<html") || lower.ends_with(".html") || lower.ends_with(".htm") {
        return Some(Sniffed::Html);
    }
    if head.contains("<svg") || lower.ends_with(".svg") {
        return Some(Sniffed::Svg);
    }
    if lower.ends_with(".toml") {
        return Some(Sniffed::Toml);
    }
    if lower.ends_with(".json") || head.starts_with('{') {
        return Some(Sniffed::Json);
    }
    None
}

/// Whether a kind moves by itself, when the item does not say.
pub fn moves(kind: GalleryKind) -> bool {
    matches!(
        kind,
        GalleryKind::Clip | GalleryKind::Html | GalleryKind::Ograf | GalleryKind::Ticker | GalleryKind::Transition | GalleryKind::Effect
    )
}

/// Whether an item is transparent anywhere, when it does not say. A clip
/// in WebM or MOV may carry alpha and an MP4 cannot.
pub fn transparent(kind: GalleryKind, file: Option<&Path>) -> bool {
    let ext = file.and_then(|f| f.extension()).map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    match kind {
        GalleryKind::Image => file.and_then(crate::plugin::kinds::image_probe::has_alpha).unwrap_or(false),
        GalleryKind::Clip | GalleryKind::Transition | GalleryKind::Effect => ext == "webm" || ext == "mov",
        GalleryKind::Set => false,
        _ => true,
    }
}

/// Where an item goes when it does not say: a set or an opaque picture
/// fills the canvas, a ticker sits at the bottom, a small transparent
/// picture is a bug, a wide one a lower third, and anything laid out on a
/// whole frame goes over the whole canvas.
pub fn zone(kind: GalleryKind, transparent: bool, size: Option<(u32, u32)>) -> Zone {
    match kind {
        GalleryKind::Set => return Zone::Full,
        GalleryKind::Ticker => return Zone::Bottom,
        GalleryKind::Text => return Zone::LowerThird,
        _ => {}
    }
    if !transparent && matches!(kind, GalleryKind::Image | GalleryKind::Clip) {
        return Zone::Full;
    }
    let Some((w, h)) = size.filter(|(w, h)| *w > 0 && *h > 0) else { return Zone::Overlay };
    let aspect = w as f64 / h as f64;
    if (aspect - 16.0 / 9.0).abs() < 0.05 && w >= 960 {
        Zone::Overlay
    } else if aspect > 3.0 {
        Zone::LowerThird
    } else if w <= 640 && h <= 640 {
        Zone::Bug
    } else {
        Zone::Center
    }
}

/// The size a picture file says it is: a PNG's header or an SVG's own.
pub fn size(path: &Path) -> Option<(u32, u32)> {
    let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
    if ext == "svg" {
        let text = std::fs::read_to_string(path).ok()?;
        return crate::plugin::kinds::image_probe::svg_size(&text);
    }
    let mut head = [0u8; 32];
    use std::io::Read;
    std::fs::File::open(path).ok()?.read_exact(&mut head).ok()?;
    if sniff(&head, "") != Some(Sniffed::Png) {
        return None;
    }
    let be = |i: usize| u32::from_be_bytes([head[i], head[i + 1], head[i + 2], head[i + 3]]);
    Some((be(16), be(20)))
}
