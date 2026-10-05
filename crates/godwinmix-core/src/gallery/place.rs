//! Where an item lands on the canvas, by its zone, and how it comes and
//! goes.
//!
//! A graphic designed on a whole frame (a template, a page, a picture the
//! canvas's shape) is placed over the whole canvas, because its own empty
//! space already puts it where it belongs. Anything smaller is placed in
//! its zone at the size it was drawn for, taking a 1920 wide canvas as the
//! one designs are made on, inside title safe.

use godwinmix_protocol::gallery::Zone;
use serde_json::{json, Value};

/// Title safe, as a share of each side: 5 percent.
const SAFE: f64 = 0.05;

/// The transform for `zone` on a canvas `w` by `h`, for an item whose own
/// size is `natural` when it has one.
pub fn transform(zone: Zone, canvas: (u32, u32), natural: Option<(u32, u32)>) -> Value {
    let (cw, ch) = (canvas.0 as f64, canvas.1 as f64);
    let whole = || json!({"position": {"x": 0, "y": 0}, "frame": {"w": canvas.0, "h": canvas.1}});
    let fills = natural.is_some_and(|(w, h)| h > 0 && ((w as f64 / h as f64) - cw / ch).abs() < 0.05);
    if matches!(zone, Zone::Full | Zone::Overlay) || fills {
        return whole();
    }
    let scale = cw / 1920.0;
    let (w, h) = match natural {
        Some((w, h)) if w > 0 && h > 0 => {
            let (w, h) = (w as f64 * scale, h as f64 * scale);
            let shrink = (cw * (1.0 - 2.0 * SAFE) / w).min(ch * 0.5 / h).min(1.0);
            (w * shrink, h * shrink)
        }
        _ => default_box(zone, cw, ch),
    };
    let (x, y) = match zone {
        Zone::LowerThird => (cw * SAFE, ch * (1.0 - SAFE) - h - ch * 0.06),
        Zone::Bug => (cw * (1.0 - SAFE) - w, ch * SAFE),
        Zone::Top => ((cw - w) / 2.0, ch * SAFE),
        Zone::Bottom => ((cw - w) / 2.0, ch * (1.0 - SAFE) - h),
        _ => ((cw - w) / 2.0, (ch - h) / 2.0),
    };
    json!({"position": {"x": x.round(), "y": y.round()}, "frame": {"w": w.round(), "h": h.round()}})
}

/// The box for an item that has no size of its own: a ticker or a text.
fn default_box(zone: Zone, cw: f64, ch: f64) -> (f64, f64) {
    match zone {
        Zone::Bottom | Zone::Top => (cw, ch * 0.075),
        Zone::LowerThird => (cw * 0.6, ch * 0.16),
        Zone::Bug => (cw * 0.15, ch * 0.12),
        _ => (cw * 0.6, ch * 0.3),
    }
}

/// How it comes on: a lower third slides in from the left, a bottom strip
/// rises, a top strip drops, anything else fades.
pub fn enter(zone: Zone) -> Value {
    match zone {
        Zone::LowerThird => json!({"type": "slide", "edge": "left", "duration_ms": 400, "easing": "ease-out"}),
        Zone::Bottom => json!({"type": "slide", "edge": "bottom", "duration_ms": 350, "easing": "ease-out"}),
        Zone::Top => json!({"type": "slide", "edge": "top", "duration_ms": 350, "easing": "ease-out"}),
        _ => json!({"type": "fade", "duration_ms": 300}),
    }
}

/// How it goes: a fade, which suits everything.
pub fn exit(_zone: Zone) -> Value {
    json!({"type": "fade", "duration_ms": 300})
}

/// True when an item in `zone` goes under everything else.
pub fn underneath(zone: Zone) -> bool {
    zone == Zone::Full
}
