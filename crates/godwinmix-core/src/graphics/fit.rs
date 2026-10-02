//! Shrink to fit: a `<text>` with `data-fit-width="W"` is made smaller when
//! the words in it come out wider than `W`, in the template's own units.
//!
//! The words are measured by the renderer that draws them, so the font,
//! weight and letter spacing are the ones on screen: see `measure`. A text
//! that fits is left exactly as written. One that does not is scaled about
//! its own `x` and `y`, the point its `text-anchor` holds, so a left aligned
//! headline keeps its left edge, a centred one its middle, and its baseline
//! stays where it was. The scale is the whole text's, height too: a long
//! headline gets smaller letters, not squashed ones.

use super::xml;
use anyhow::Result;

/// The attribute that asks for it.
pub const ATTR: &str = "data-fit-width";

/// One `<text>` asking to fit: where its start tag is, and its width.
struct FitText {
    start: usize,
    end: usize,
    width: f64,
    content: (usize, usize),
}

fn fit_texts(svg: &str) -> Vec<FitText> {
    xml::tags(svg, "text")
        .into_iter()
        .filter_map(|t| {
            let width = xml::attr(t.text, ATTR)?.trim().trim_end_matches("px").parse::<f64>().ok().filter(|w| *w > 0.0)?;
            let close = svg[t.end..].find("</text>").map_or(t.end, |i| t.end + i);
            Some(FitText { start: t.start, end: t.end, width, content: (t.end, close) })
        })
        .collect()
}

/// Each field written inside a fitted text, with the width it fits in.
pub fn fitted_fields(svg: &str) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for t in fit_texts(svg) {
        for m in super::template::markers(&svg[t.content.0..t.content.1]) {
            out.push((m.name, t.width));
        }
    }
    out
}

/// `svg` (filled) with every fitted text that is too wide scaled down.
/// `measure` says how wide the text with the given id is, in the
/// template's units; it is called once per fitted text.
pub fn shrink(svg: &str, measure: &mut dyn FnMut(&str, &str) -> Result<f64>) -> Result<String> {
    let texts = fit_texts(svg);
    if texts.is_empty() {
        return Ok(svg.to_string());
    }
    // Every fitted text gets an id first, so the measuring copy can show
    // that one alone.
    let mut named = String::with_capacity(svg.len() + 64);
    let mut ids = Vec::new();
    let mut at = 0;
    for (i, t) in texts.iter().enumerate() {
        let tag = &svg[t.start..t.end];
        let id = xml::attr(tag, "id").unwrap_or_else(|| format!("gmx-fit-{i}"));
        named.push_str(&svg[at..t.start]);
        named.push_str(&xml::set_attr(tag, "id", &id));
        at = t.end;
        ids.push(id);
    }
    named.push_str(&svg[at..]);
    let mut out = String::with_capacity(named.len() + 128);
    let mut at = 0;
    for (t, id) in fit_texts(&named).iter().zip(&ids) {
        let tag = &named[t.start..t.end];
        let wide = measure(&named, id)?;
        out.push_str(&named[at..t.start]);
        if wide > t.width {
            out.push_str(&scaled(tag, t.width / wide));
        } else {
            out.push_str(tag);
        }
        at = t.end;
    }
    out.push_str(&named[at..]);
    Ok(out)
}

/// The start tag of a text scaled by `k` about its own `x` and `y`. Any
/// transform it had stays outermost.
fn scaled(tag: &str, k: f64) -> String {
    let first = |name: &str| {
        xml::attr(tag, name).and_then(|v| v.split([' ', ',']).find(|s| !s.is_empty()).and_then(|s| s.trim_end_matches("px").parse::<f64>().ok())).unwrap_or(0.0)
    };
    let (x, y) = (first("x"), first("y"));
    let own = format!("translate({x} {y}) scale({k:.4}) translate({} {})", -x, -y);
    let transform = match xml::attr(tag, "transform") {
        Some(t) if !t.trim().is_empty() => format!("{t} {own}"),
        _ => own,
    };
    xml::set_attr(tag, "transform", &transform)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = r#"<svg viewBox="0 0 100 20"><text x="10" y="15" data-fit-width="50">{{headline}}</text><text>{{other}}</text></svg>"#;

    #[test]
    fn the_fields_inside_a_fitted_text_are_named_with_its_width() {
        assert_eq!(fitted_fields(DOC), vec![("headline".to_string(), 50.0)]);
    }

    #[test]
    fn a_text_that_fits_is_untouched_and_one_that_does_not_is_scaled_about_its_anchor() {
        let filled = DOC.replace("{{headline}}", "Short");
        let out = shrink(&filled, &mut |_, _| Ok(40.0)).unwrap();
        assert!(!out.contains("transform"), "{out}");
        let out = shrink(&filled, &mut |_, id| {
            assert_eq!(id, "gmx-fit-0");
            Ok(100.0)
        })
        .unwrap();
        assert!(out.contains(r#"transform="translate(10 15) scale(0.5000) translate(-10 -15)""#), "{out}");
    }
}
