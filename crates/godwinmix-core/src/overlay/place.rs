//! Where a layer is drawn, read off the compositor pad the scene wrote.
//!
//! The scene machinery does not know a transparent item is drawn anywhere
//! but the compositor: it binds the item to a slot and writes `xpos`, `ypos`,
//! `width`, `height`, `alpha`, `zorder` and `sizing-policy` on that slot's pad
//! like it does for every item, and a transition drives the same properties.
//! The board reads them back each frame. So place, size, stacking and fades
//! all come from the one place every other item's do.

use super::blend::{Draw, Rect};
use super::picture::{Direction, Motion, Picture};
use gstreamer as gst;
use gstreamer::glib;
use gstreamer::prelude::*;

/// One item's box on the canvas, as its pad says this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PadBox {
    pub rect: Rect,
    pub alpha: f64,
    pub z: u32,
    pub fit: Fit,
    /// Where a picture that does not fill the box sits in it, 0 to 1 each.
    pub align: (f64, f64),
}

/// The three sizing policies a compositor pad has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    Fill,
    Contain,
    Cover,
}

/// The box a pad draws in, or `None` for a pad that is hidden.
pub fn read(pad: &gst::Pad, canvas: (i32, i32)) -> Option<PadBox> {
    let alpha: f64 = pad.property("alpha");
    if alpha <= 0.0 {
        return None;
    }
    let w: i32 = pad.property("width");
    let h: i32 = pad.property("height");
    let rect = Rect::new(
        pad.property("xpos"),
        pad.property("ypos"),
        if w > 0 { w } else { canvas.0 },
        if h > 0 { h } else { canvas.1 },
    );
    let align = |name: &str| if pad.has_property(name) { pad.property::<f64>(name) } else { 0.5 };
    Some(PadBox { rect, alpha: alpha.min(1.0), z: pad.property("zorder"), fit: fit_of(pad), align: (align("xalign"), align("yalign")) })
}

fn fit_of(pad: &gst::Pad) -> Fit {
    if pad.find_property("sizing-policy").is_none() {
        return Fit::Fill;
    }
    let held = pad.property_value("sizing-policy");
    match glib::EnumValue::from_value(&held).map(|(_, v)| v.nick()) {
        Some("keep-aspect-ratio") => Fit::Contain,
        Some("keep-aspect-ratio-with-crop") => Fit::Cover,
        _ => Fit::Fill,
    }
}

/// The rectangle a picture of shape `natural` takes inside `boxed`, placed
/// by `align` where it does not fill it.
pub fn fitted(boxed: Rect, natural: (u32, u32), fit: Fit, align: (f64, f64)) -> Rect {
    let (nw, nh) = (natural.0.max(1) as f64, natural.1.max(1) as f64);
    let (bw, bh) = (boxed.w as f64, boxed.h as f64);
    let scale = match fit {
        Fit::Fill => return boxed,
        Fit::Contain => (bw / nw).min(bh / nh),
        Fit::Cover => (bw / nw).max(bh / nh),
    };
    let (w, h) = ((nw * scale).round() as i32, (nh * scale).round() as i32);
    let at = |room: i32, a: f64| (room as f64 * a.clamp(0.0, 1.0)).round() as i32;
    Rect::new(boxed.x + at(boxed.w - w, align.0), boxed.y + at(boxed.h - h, align.1), w.max(1), h.max(1))
}

/// How to draw a held picture in a box this frame, and the size it is drawn
/// at, which is the size the kind should render it at next.
pub fn still(pic: &Picture, b: &PadBox) -> (Draw, (u32, u32)) {
    let to = within(fitted(b.rect, pic.natural, b.fit, b.align), pic);
    let draw = Draw { window: Rect::new(0, 0, pic.width as i32, pic.height as i32), to, clip: b.rect, alpha: alpha8(b.alpha) };
    (draw, (to.w as u32, to.h as u32))
}

/// The part of `to` a picture covering only some of its natural frame
/// lands in.
fn within(to: Rect, pic: &Picture) -> Rect {
    let Some(a) = pic.within else { return to };
    let (nw, nh) = (pic.natural.0.max(1) as i64, pic.natural.1.max(1) as i64);
    let x = to.x + (a.x as i64 * to.w as i64 / nw) as i32;
    let y = to.y + (a.y as i64 * to.h as i64 / nh) as i32;
    let r = to.x + ((a.x + a.w) as i64 * to.w as i64 / nw) as i32;
    let bottom = to.y + ((a.y + a.h) as i64 * to.h as i64 / nh) as i32;
    Rect::new(x, y, (r - x).max(1), (bottom - y).max(1))
}

/// A crawl: the strip at its own size, moved through the box and cut off at
/// its edges. `travelled` is how far it has moved since it started, in
/// pixels. Returns one draw per copy of the strip in view, and the box size,
/// which is what the kind renders the strip's height (or width) for.
pub fn crawl(pic: &Picture, b: &PadBox, motion: Motion, travelled: f64) -> (Vec<Draw>, (u32, u32)) {
    let Motion::Crawl { direction, gap, repeat, speed } = motion else { return (Vec::new(), (0, 0)) };
    let (w, h) = (pic.width as i32, pic.height as i32);
    let span = match direction {
        Direction::Up => b.rect.h + h,
        _ => b.rect.w + w,
    };
    let period = match direction {
        Direction::Up => h + gap as i32,
        _ => w + gap as i32,
    }
    .max(1);
    // Stopped, the strip stands at the near edge of the box, readable, rather
    // than just outside it where a crawl starts.
    let t = if speed <= 0.0 {
        match direction {
            Direction::Left => b.rect.w as i64,
            Direction::Right => w as i64,
            Direction::Up => b.rect.h as i64,
        }
    } else {
        travelled.max(0.0) as i64
    };
    let at = |offset: i64| -> Rect {
        let o = offset as i32;
        match direction {
            Direction::Left => Rect::new(b.rect.right() - o, b.rect.y + (b.rect.h - h) / 2, w, h),
            Direction::Right => Rect::new(b.rect.x - w + o, b.rect.y + (b.rect.h - h) / 2, w, h),
            Direction::Up => Rect::new(b.rect.x + (b.rect.w - w) / 2, b.rect.bottom() - o, w, h),
        }
    };
    let window = Rect::new(0, 0, w, h);
    let alpha = alpha8(b.alpha);
    // Every copy that has entered so far, newest first: the newest has moved
    // `t % period`, the one before it a period further, and so on until one
    // has gone out of the far side. Without `repeat` there is only the first.
    let (copies, lead) = if repeat && speed > 0.0 { (t / period as i64 + 1, t % period as i64) } else { (1, t) };
    let draws = (0..copies.min(64))
        .map(|k| lead + k * period as i64)
        .take_while(|offset| *offset < span as i64)
        .map(|offset| Draw { window, to: at(offset), clip: b.rect, alpha })
        .collect();
    (draws, (b.rect.w as u32, b.rect.h as u32))
}

fn alpha8(alpha: f64) -> u8 {
    (alpha.clamp(0.0, 1.0) * 255.0).round() as u8
}
