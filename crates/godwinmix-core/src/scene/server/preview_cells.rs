//! The armed scene's placements as the preview draws them.
//!
//! One function for the armed scene and for a draft shown in its place, so
//! the two cannot drift. Each cell carries its item's filters, because the
//! preview has to show a keyed presenter keyed: it used to carry only the
//! geometry, and Preview showed the green screen that Programme keyed out.
//!
//! A group with a filter is one placement on the programme, composited on its
//! own and filtered as one picture (`mixer::group`). The preview has no sub
//! compositor and skipped the group altogether. It now draws the group's
//! children with the group's filters after their own, which is the same
//! picture for every filter that works pixel by pixel (a look, a colour
//! correction) and for a key over children that do not overlap.

use super::{PreviewCell, Placement};

/// `placements` in canvas pixels, scaled by `sx` and `sy`.
pub(super) fn cells(placements: Vec<Placement>, sx: f64, sy: f64) -> Vec<PreviewCell> {
    let mut out = Vec::with_capacity(placements.len());
    for p in placements {
        if p.group.is_empty() {
            out.push(cell(p, sx, sy));
            continue;
        }
        let outer = p.filters;
        for mut kid in p.group {
            kid.filters.extend(outer.iter().cloned());
            out.push(cell(kid, sx, sy));
        }
    }
    out
}

fn cell(p: Placement, sx: f64, sy: f64) -> PreviewCell {
    PreviewCell {
        source: p.source,
        x: (p.xpos as f64 * sx).round() as i32,
        y: (p.ypos as f64 * sy).round() as i32,
        width: (p.width as f64 * sx).round() as i32,
        height: (p.height as f64 * sy).round() as i32,
        alpha: p.alpha,
        rotation: p.rotation,
        crop: p.crop,
        fit: p.sizing.name().to_string(),
        filters: p.filters,
    }
}
