//! Whether a picture has an alpha channel, read from the first bytes of the
//! file rather than by decoding it, and how big an SVG says it is.
//!
//! The answer decides which way the picture goes: an opaque one through the
//! compositor like any source, a transparent one to the overlay board. Read
//! once, when the source is built.

use std::io::Read;
use std::path::Path;

/// What a local picture file says about transparency. `None` when the file
/// cannot be read or is not a format this knows.
pub fn has_alpha(path: &Path) -> Option<bool> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    if ext == "svg" {
        return Some(true);
    }
    let mut head = vec![0u8; 64 * 1024];
    let n = std::fs::File::open(path).ok()?.read(&mut head).ok()?;
    head.truncate(n);
    match ext.as_str() {
        "png" => png_alpha(&head),
        "webp" => webp_alpha(&head),
        _ => Some(false),
    }
}

/// Colour type 4 (grey and alpha) or 6 (RGBA) in the header, or a `tRNS`
/// chunk before the first image data.
fn png_alpha(b: &[u8]) -> Option<bool> {
    if b.len() < 33 || &b[1..4] != b"PNG" {
        return None;
    }
    if matches!(b[25], 4 | 6) {
        return Some(true);
    }
    let mut at = 8;
    while at + 8 <= b.len() {
        let len = u32::from_be_bytes([b[at], b[at + 1], b[at + 2], b[at + 3]]) as usize;
        match &b[at + 4..at + 8] {
            b"tRNS" => return Some(true),
            b"IDAT" | b"IEND" => return Some(false),
            _ => at += 12 + len,
        }
    }
    Some(false)
}

/// The alpha flag of an extended WebP, or the alpha hint of a lossless one.
fn webp_alpha(b: &[u8]) -> Option<bool> {
    if b.len() < 30 || &b[0..4] != b"RIFF" || &b[8..12] != b"WEBP" {
        return None;
    }
    Some(match &b[12..16] {
        b"VP8X" => b[20] & 0x10 != 0,
        b"VP8L" => b.len() > 24 && b[24] & 0x10 != 0,
        _ => false,
    })
}

/// The size an SVG's root element declares: `width` and `height` when they
/// are plain numbers or pixels, otherwise the `viewBox`.
pub fn svg_size(text: &str) -> Option<(u32, u32)> {
    let start = text.find("<svg")?;
    let tag = &text[start..start + text[start..].find('>')?];
    let attr = |name: &str| -> Option<String> {
        let at = tag.find(&format!(" {name}="))? + name.len() + 2;
        let quote = tag[at..].chars().next()?;
        let rest = &tag[at + 1..];
        Some(rest[..rest.find(quote)?].to_string())
    };
    let px = |v: String| v.trim().trim_end_matches("px").parse::<f64>().ok().filter(|n| *n > 0.0);
    if let (Some(w), Some(h)) = (attr("width").and_then(px), attr("height").and_then(px)) {
        return Some((w.round() as u32, h.round() as u32));
    }
    let vb: Vec<f64> = attr("viewBox")?.split([' ', ',']).filter_map(|n| n.parse().ok()).collect();
    (vb.len() == 4 && vb[2] > 0.0 && vb[3] > 0.0).then(|| (vb[2].round() as u32, vb[3].round() as u32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_sizes_come_from_width_and_height_or_the_view_box() {
        assert_eq!(svg_size(r#"<?xml?><svg xmlns="x" width="200px" height="80">"#), Some((200, 80)));
        assert_eq!(svg_size(r#"<svg viewBox="0 0 640 360" width="100%">"#), Some((640, 360)));
        assert_eq!(svg_size("<html>"), None);
    }

    #[test]
    fn a_png_header_says_whether_it_has_alpha() {
        let mut rgba = vec![0x89, b'P', b'N', b'G', 13, 10, 26, 10, 0, 0, 0, 13];
        rgba.extend(b"IHDR");
        rgba.extend([0, 0, 0, 4, 0, 0, 0, 4, 8, 6, 0, 0, 0]);
        rgba.extend([0; 8]);
        assert_eq!(png_alpha(&rgba), Some(true));
        let mut rgb = rgba.clone();
        rgb[25] = 2;
        assert_eq!(png_alpha(&rgb), Some(false));
    }
}
