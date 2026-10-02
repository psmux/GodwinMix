//! How wide a text comes out, asked of the renderer that draws it.
//!
//! A copy of the filled template is drawn with every element hidden but the
//! one text, on a canvas widened to five times the template's width so a
//! long headline is not cut at the edge, at a scale that keeps the canvas
//! under `MAX_SIDE` pixels across. The width is the span of columns with any
//! ink in them. librsvg lays the words out with Pango, the same as when the
//! template is drawn for real, so the measure is the font on screen and not
//! a guess from the number of letters.
//!
//! Remembered by the exact document and text, so a graphic resized in the
//! composer, or a field changed elsewhere in it, measures nothing again.

use super::xml;
use crate::overlay::Picture;
use crate::plugin::kinds::image_decode::svg_data;
use anyhow::{Context, Result};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

/// The widest a measuring canvas is drawn, in pixels.
const MAX_SIDE: f64 = 4000.0;
/// How many measures are remembered before the oldest are forgotten.
const REMEMBERED: usize = 256;

static SEEN: Mutex<Option<HashMap<u64, f64>>> = Mutex::new(None);

/// The width of the text with id `id` in `svg`, in the template's units.
pub fn width(svg: &str, id: &str) -> Result<f64> {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    (svg, id).hash(&mut h);
    let key = h.finish();
    if let Some(w) = SEEN.lock().as_ref().and_then(|m| m.get(&key).copied()) {
        return Ok(w);
    }
    let w = render_width(svg, id)?;
    let mut seen = SEEN.lock();
    let map = seen.get_or_insert_with(HashMap::new);
    if map.len() >= REMEMBERED {
        map.clear();
    }
    map.insert(key, w);
    Ok(w)
}

fn render_width(svg: &str, id: &str) -> Result<f64> {
    let root = xml::tags(svg, "svg").into_iter().next().context("a template with no <svg> element")?;
    let (min_x, min_y, w, h) = view_box(root.text).context("a template with no size; give its <svg> a viewBox")?;
    let scale = (MAX_SIDE / (5.0 * w)).min(1.0);
    let px = (even(5.0 * w * scale), even(h * scale));
    let mut tag = root.text.to_string();
    for (name, value) in [
        ("viewBox", format!("{} {min_y} {} {h}", min_x - 2.0 * w, 5.0 * w)),
        ("width", px.0.to_string()),
        ("height", px.1.to_string()),
        ("preserveAspectRatio", "none".to_string()),
    ] {
        tag = xml::set_attr(&tag, name, &value);
    }
    let only = format!("<style>*{{visibility:hidden !important}}#{id},#{id} *{{visibility:visible !important}}</style>");
    let doc = format!("{}{tag}{only}{}", &svg[..root.start], &svg[root.end..]);
    let pic = svg_data(&doc, px)?;
    Ok(inked_columns(&pic) as f64 / (px.0 as f64 / (5.0 * w)))
}

/// The root's `viewBox`, or its width and height from the origin.
fn view_box(root: &str) -> Option<(f64, f64, f64, f64)> {
    let nums = |v: String| -> Vec<f64> { v.split([' ', ',']).filter_map(|n| n.trim_end_matches("px").parse().ok()).collect() };
    if let Some(vb) = xml::attr(root, "viewBox").map(nums).filter(|v| v.len() == 4 && v[2] > 0.0 && v[3] > 0.0) {
        return Some((vb[0], vb[1], vb[2], vb[3]));
    }
    let w = xml::attr(root, "width").map(nums)?.first().copied()?;
    let h = xml::attr(root, "height").map(nums)?.first().copied()?;
    Some((0.0, 0.0, w, h))
}

fn even(v: f64) -> u32 {
    ((v.round() as u32).max(2) + 1) & !1
}

/// From the first column with any ink to the last, in pixels.
fn inked_columns(pic: &Picture) -> u32 {
    let Ok(map) = pic.buffer.map_readable() else { return 0 };
    let (mut first, mut last) = (u32::MAX, 0u32);
    for y in 0..pic.height as usize {
        let row = &map[y * pic.stride..y * pic.stride + pic.width as usize * 4];
        if let Some(a) = row.chunks_exact(4).position(|px| px[0] > 8) {
            first = first.min(a as u32);
            last = last.max(row.chunks_exact(4).rposition(|px| px[0] > 8).unwrap_or(a) as u32);
        }
    }
    if first == u32::MAX {
        0
    } else {
        last - first + 1
    }
}
