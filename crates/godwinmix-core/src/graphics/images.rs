//! Pictures inside a template: a logo as a data URI, or a file in the media
//! library named by its name. Nothing is fetched from the network, ever: a
//! graphic that waits on a server is a graphic that is late on air.
//!
//! A library file is read and put in as a data URI when the template is
//! rendered, because `rsvgdec` is given the SVG as bytes, with no folder to
//! look in.

use anyhow::{bail, Context, Result};
use std::path::Path;

/// One `href` value: where it sits in the document and what it says.
struct Href {
    start: usize,
    end: usize,
    value: String,
}

/// Every `href` and `xlink:href` value in `svg`.
fn hrefs(svg: &str) -> Vec<Href> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = svg[from..].find("href=").map(|i| i + from) {
        from = at + 5;
        let before = svg[..at].chars().next_back();
        if !matches!(before, Some(c) if c.is_whitespace() || c == ':') {
            continue;
        }
        let Some(quote) = svg[from..].chars().next().filter(|q| *q == '"' || *q == '\'') else { continue };
        let Some(len) = svg[from + 1..].find(quote) else { break };
        out.push(Href { start: from + 1, end: from + 1 + len, value: svg[from + 1..from + 1 + len].trim().to_string() });
        from += len + 2;
    }
    out
}

fn is_remote(v: &str) -> bool {
    v.contains("://") || v.starts_with("//")
}

/// Refuse a template that names anything on the network.
pub fn check_refs(name: &str, svg: &str) -> Result<()> {
    let remote = hrefs(svg).into_iter().map(|h| h.value).find(|v| is_remote(v) && !v.starts_with("data:"));
    let css = svg.contains("@import") || svg.split("url(").skip(1).any(|rest| is_remote(rest.split(')').next().unwrap_or("")));
    if let Some(v) = remote {
        bail!("template {name} refers to {v}, and a template is never fetched from the network as it renders. Embed the picture as a data: URI, or upload it to the media library and refer to it by its file name");
    }
    if css {
        bail!("template {name} imports a style or a font from another address. Use fonts installed on the mixer, and put the styles inside the SVG");
    }
    Ok(())
}

/// `svg` with every library file it names put in as a data URI.
pub fn inline(svg: &str, library: Option<&Path>) -> Result<String> {
    let local: Vec<Href> = hrefs(svg).into_iter().filter(|h| !h.value.is_empty() && !h.value.starts_with('#') && !h.value.starts_with("data:") && !is_remote(&h.value)).collect();
    if local.is_empty() {
        return Ok(svg.to_string());
    }
    let mut out = String::with_capacity(svg.len());
    let mut at = 0;
    for h in local {
        out.push_str(&svg[at..h.start]);
        out.push_str(&data_uri(&h.value, library)?);
        at = h.end;
    }
    out.push_str(&svg[at..]);
    Ok(out)
}

/// The library file `name` as a data URI.
fn data_uri(name: &str, library: Option<&Path>) -> Result<String> {
    let lib = library.context("this template names a picture in the media library, and this mixer has no media library")?;
    let name = super::xml::unescape(name.trim_start_matches("media:"));
    if name.split(['/', '\\']).any(|part| part == ".." || part.is_empty()) || Path::new(&name).is_absolute() {
        bail!("a template's picture {name:?} is named by its file name in the media library, with no .. and no leading /");
    }
    let path = lib.join(&name);
    let bytes = std::fs::read(&path).with_context(|| format!("the template's picture {name} is not in the media library at {}. Upload it, or embed it as a data: URI", path.display()))?;
    let mime = match path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref() {
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        _ => bail!("a template's picture {name} is not a PNG, JPEG, GIF, WebP or SVG"),
    };
    Ok(format!("data:{mime};base64,{}", glib::base64_encode(&bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_network_reference_is_refused_and_a_data_uri_is_not() {
        let ok = r##"<svg xmlns="http://www.w3.org/2000/svg"><image href="data:image/png;base64,AAAA"/><use href="#a"/></svg>"##;
        assert!(check_refs("t", ok).is_ok(), "the xmlns is not a reference");
        let bad = r#"<svg><image xlink:href="https://example.com/logo.png"/></svg>"#;
        let e = check_refs("t", bad).unwrap_err().to_string();
        assert!(e.contains("media library") && e.contains("data:"), "{e}");
        assert!(check_refs("t", "<svg><style>@import url(https://f.example/x.css);</style></svg>").is_err());
    }

    #[test]
    fn a_library_picture_is_put_in_as_data() {
        let dir = std::env::temp_dir().join(format!("gmx-tpl-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("logo.png"), [1u8, 2, 3]).unwrap();
        let out = inline(r#"<svg><image href="logo.png"/></svg>"#, Some(&dir)).unwrap();
        assert_eq!(out, r#"<svg><image href="data:image/png;base64,AQID"/></svg>"#);
        assert!(inline(r#"<image href="../etc/passwd.png"/>"#, Some(&dir)).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
