//! Reading an HTML template the way the renderer will, before it is drawn,
//! and saying what to change when it would not work.
//!
//! These are the mistakes a model makes most: a page that paints its own
//! background over the picture, a field shown on the page and never
//! declared, a font or a script from the network, a graphic with no way in.
//! Each answer quotes what it found and says what to write instead.

use super::fields::{fields, motion};
use super::meta::{self, Meta};
use super::problem::{error, Problems};
use crate::graphics::template::MAX_BYTES;

/// Every problem with `html`, and the declaration when it reads.
pub fn check(html: &str) -> (Problems, Option<Meta>) {
    let mut out = Problems::new();
    if html.len() > MAX_BYTES {
        out.push(error(&format!("the page is {} bytes", html.len()), &format!("keep a template under {MAX_BYTES} bytes: put a large picture in the media library and name it by file name")));
        return (out, None);
    }
    if !html.to_ascii_lowercase().contains("<html") && !html.to_ascii_lowercase().contains("<body") {
        out.push(error("this is not an HTML page", "start from a pack template (get_template with a name from list_templates) and change it"));
        return (out, None);
    }
    let meta = meta::read(html, &mut out);
    let page = without_block(html);
    if !meta.as_ref().is_some_and(|m| m.opaque) {
        background(&page, &mut out);
    }
    network(&page, &mut out);
    dialogs(&page, &mut out);
    if let Some(m) = &meta {
        fields(&page, m, &mut out);
        motion(&page, m, &mut out);
    }
    (out, meta)
}

/// The page with its declaration taken out, so the words in the block are
/// not mistaken for the page's own.
fn without_block(html: &str) -> String {
    match meta::block(html) {
        Some(block) => html.replacen(block, "", 1),
        None => html.to_string(),
    }
}

/// A background on html, body or :root that is not transparent.
fn background(page: &str, out: &mut Problems) {
    for (selector, value) in root_backgrounds(page) {
        if !clear(&value) {
            out.push(error(
                &format!("the page paints its background ({selector} {{ background: {value} }}), which covers the picture under the graphic"),
                "write html, body { background: transparent; margin: 0 } and put colour only on the parts of the graphic (the panel, the bar). If the design is meant to fill the screen, a background or a title card, add \"opaque\": true to the gmx-template block",
            ));
        }
    }
}

/// Every `background` or `background-color` given to html, body or :root,
/// in a style sheet or in the tag's own style attribute.
pub fn root_backgrounds(page: &str) -> Vec<(String, String)> {
    let lower = page.to_ascii_lowercase();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(open) = lower[from..].find('{').map(|i| i + from) {
        let Some(close) = lower[open..].find('}').map(|i| i + open) else { break };
        let start = lower[..open].rfind(['}', '>', ';', '{']).map(|i| i + 1).unwrap_or(0);
        let selectors = &lower[start..open];
        let root = selectors.split(',').map(str::trim).find(|s| matches!(*s, "html" | "body" | ":root"));
        if let Some(sel) = root {
            for value in backgrounds_in(&page[open + 1..close]) {
                found.push((sel.to_string(), value));
            }
        }
        from = close + 1;
    }
    for tag in ["<html", "<body"] {
        if let Some(at) = lower.find(tag) {
            let end = lower[at..].find('>').map(|i| i + at).unwrap_or(lower.len());
            if let Some(s) = lower[at..end].find("style=").map(|i| i + at + 6) {
                let q = &page[s..end];
                found.extend(backgrounds_in(q.trim_matches(['"', '\''])).into_iter().map(|v| (tag[1..].to_string(), v)));
            }
        }
    }
    found
}

/// The values of `background` and `background-color` in a declaration list.
fn backgrounds_in(decls: &str) -> Vec<String> {
    decls
        .split(';')
        .filter_map(|d| d.split_once(':'))
        .filter(|(k, _)| matches!(k.trim().to_ascii_lowercase().as_str(), "background" | "background-color"))
        .map(|(_, v)| v.trim().trim_matches(['"', '\'']).to_string())
        .collect()
}

/// Whether a background value lets the picture through.
fn clear(value: &str) -> bool {
    let v = value.to_ascii_lowercase().replace("!important", "").replace(' ', "");
    if matches!(v.as_str(), "transparent" | "none" | "initial" | "unset" | "inherit" | "#0000" | "#00000000") {
        return true;
    }
    (v.starts_with("rgba(") || v.starts_with("hsla(") || v.starts_with("rgb(") || v.starts_with("hsl("))
        && (v.ends_with(",0)") || v.ends_with("/0)") || v.ends_with(",0.0)") || v.ends_with("/0%)"))
}

/// Anything fetched from the network: at show time there may be none.
fn network(page: &str, out: &mut Problems) {
    let mut urls: Vec<String> = Vec::new();
    for scheme in ["http://", "https://", "//fonts.googleapis", "//cdn"] {
        let mut from = 0;
        while let Some(at) = page[from..].find(scheme).map(|i| i + from) {
            let end = page[at..].find(|c: char| c.is_whitespace() || "\"')<>".contains(c)).map(|i| i + at).unwrap_or(page.len());
            let url = &page[at..end];
            if !url.starts_with("http://www.w3.org/") && !urls.iter().any(|u| u == url) {
                urls.push(url.to_string());
            }
            from = end.max(at + scheme.len());
        }
    }
    if let Some(first) = urls.first() {
        let more = if urls.len() > 1 { format!(" and {} more", urls.len() - 1) } else { String::new() };
        out.push(error(
            &format!("the page loads {first}{more} from the network, and a graphic is drawn offline: nothing from the network arrives"),
            "use a font the mixer has (Inter, Helvetica, Arial, DejaVu Sans, sans-serif), write the script into the page, and put pictures and font files in the media library, named by file name such as src=\"logo.png\"",
        ));
    }
}

/// `alert`, `confirm` and `prompt` wait for a person who is not there.
fn dialogs(page: &str, out: &mut Problems) {
    if let Some(d) = ["alert(", "confirm(", "prompt("].into_iter().find(|d| page.contains(d)) {
        out.push(error(&format!("the page calls {d}...), which waits for a click nobody will make"), "take it out; log with console.log if you need to"));
    }
}

#[cfg(test)]
#[path = "check_tests.rs"]
mod tests;
