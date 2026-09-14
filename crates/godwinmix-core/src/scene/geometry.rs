//! Where an item actually lands on the canvas.
//!
//! One function computes an item's rectangle and everything else uses it: the
//! validator that reports overlaps and off canvas items, the layout tests that
//! check every preset at two canvas sizes, and the importer's tests that assert
//! an OBS item arrived where it was. Two implementations of this would disagree
//! eventually, so there is one.

use super::document::{Canvas, Crop, Fit, Frame, Item, Transform, Vec2};

/// An axis aligned rectangle in canvas pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn new(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    /// The whole canvas.
    pub fn of(canvas: &Canvas) -> Rect {
        Rect::new(0.0, 0.0, canvas.width as f64, canvas.height as f64)
    }

    pub fn right(&self) -> f64 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f64 {
        self.y + self.h
    }

    pub fn area(&self) -> f64 {
        (self.w * self.h).max(0.0)
    }

    /// The overlapping part of two rectangles, or `None` when they miss.
    pub fn intersect(&self, other: &Rect) -> Option<Rect> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let w = self.right().min(other.right()) - x;
        let h = self.bottom().min(other.bottom()) - y;
        (w > EPSILON && h > EPSILON).then_some(Rect::new(x, y, w, h))
    }

    /// True when `self` is inside `outer`, give or take a rounding error.
    pub fn inside(&self, outer: &Rect) -> bool {
        self.x >= outer.x - EPSILON
            && self.y >= outer.y - EPSILON
            && self.right() <= outer.right() + EPSILON
            && self.bottom() <= outer.bottom() + EPSILON
    }

    /// Shrink towards the centre by a fraction of each side: 0.1 leaves the
    /// middle 90 percent, which is the title safe box.
    pub fn inset_fraction(&self, fraction: f64) -> Rect {
        let (dx, dy) = (self.w * fraction / 2.0, self.h * fraction / 2.0);
        Rect::new(self.x + dx, self.y + dy, self.w - dx * 2.0, self.h - dy * 2.0)
    }

    /// Rounded to whole pixels, for a message a person reads.
    pub fn rounded(&self) -> (i64, i64, i64, i64) {
        (
            self.x.round() as i64,
            self.y.round() as i64,
            self.w.round() as i64,
            self.h.round() as i64,
        )
    }
}

/// Half a pixel of slack. Everything here is floating point and a test that
/// demands exactness fails on arithmetic, not on a bug.
pub const EPSILON: f64 = 0.5;

/// The box an item occupies, before any rotation.
///
/// `frame` decides the size when the item has one, which is the normal case
/// after an import or a layout. Without a frame the item is the content's own
/// size scaled, and since the document does not know a source's size, the
/// canvas stands in for it; the validator says so when it matters.
pub fn item_rect(transform: &Transform, canvas: &Canvas) -> Rect {
    let (w, h) = match transform.frame {
        Some(Frame { w, h }) => (w, h),
        None => (canvas.width as f64, canvas.height as f64),
    };
    let (w, h) = (w * transform.scale.x, h * transform.scale.y);
    Rect::new(
        transform.position.x - transform.anchor.x * w,
        transform.position.y - transform.anchor.y * h,
        w,
        h,
    )
}

/// The rectangle content of a given size lands in inside `frame`, under a fit
/// and an alignment. This is the arithmetic `sizing-policy`, `xalign` and
/// `yalign` do on a `glvideomixer` pad, written out so a client can draw the
/// same picture without a pipeline.
pub fn fitted(frame: Rect, content: (f64, f64), fit: Fit, align: super::document::Align) -> Rect {
    let (cw, ch) = content;
    if cw <= 0.0 || ch <= 0.0 {
        return frame;
    }
    let (sx, sy) = (frame.w / cw, frame.h / ch);
    let scale = match fit {
        Fit::None => 1.0,
        Fit::Contain => sx.min(sy),
        Fit::Cover => sx.max(sy),
        Fit::Stretch => return frame,
        Fit::FitWidth => sx,
        Fit::FitHeight => sy,
        Fit::Max => sx.min(sy).min(1.0),
    };
    let (w, h) = (cw * scale, ch * scale);
    let f = align.factors();
    Rect::new(frame.x + (frame.w - w) * f.x, frame.y + (frame.h - h) * f.y, w, h)
}

/// Compose a parent transform with a child's, giving the child's transform in
/// the parent's coordinate space.
///
/// A group's transform multiplies into each child and its opacity into each
/// child's alpha, which is what 11 section 2 means by flattening a group at
/// apply time. Doing it here, once, is what removes the class of bug OBS
/// collected across issues 2913, 4173, 5478, 9297, 9298 and 9558.
pub fn compose(parent: &Transform, child: &Transform) -> Transform {
    let scale = Vec2::new(parent.scale.x * child.scale.x, parent.scale.y * child.scale.y);
    let offset = Vec2::new(child.position.x * parent.scale.x, child.position.y * parent.scale.y);
    let theta = parent.rotation.to_radians();
    let (sin, cos) = theta.sin_cos();
    let rotated = Vec2::new(offset.x * cos - offset.y * sin, offset.x * sin + offset.y * cos);
    let frame = child.frame.map(|f| Frame::new(f.w, f.h));
    Transform {
        position: Vec2::new(parent.position.x + rotated.x, parent.position.y + rotated.y),
        rotation: parent.rotation + child.rotation,
        scale,
        anchor: child.anchor,
        frame,
        fit: child.fit,
        align: child.align,
    }
}

/// One item as the compositor will see it, after groups are flattened.
#[derive(Debug, Clone)]
pub struct Placement<'a> {
    pub item: &'a Item,
    /// The item's transform composed with every group above it.
    pub transform: Transform,
    /// The item's opacity multiplied by every group's above it.
    pub opacity: f64,
    /// Where it lands.
    pub rect: Rect,
    /// The path of names from the top item down, for a message.
    pub path: String,
}

/// Flatten a scene's items into the list the compositor would be given:
/// groups multiplied into their children, invisible items dropped, bottom of
/// the stack first.
pub fn flatten<'a>(items: &'a [Item], canvas: &Canvas) -> Vec<Placement<'a>> {
    let mut out = Vec::new();
    walk(items, &Transform::default(), 1.0, "", canvas, &mut out);
    out
}

fn walk<'a>(
    items: &'a [Item],
    parent: &Transform,
    opacity: f64,
    path: &str,
    canvas: &Canvas,
    out: &mut Vec<Placement<'a>>,
) {
    for item in items {
        if !item.visible {
            continue;
        }
        let transform = compose(parent, &item.transform);
        let alpha = opacity * item.opacity;
        let label = item.name.clone().unwrap_or_else(|| item.id.to_string());
        let here = if path.is_empty() { label } else { format!("{path} / {label}") };
        match &item.content {
            super::document::Content::Children { children } => {
                walk(children, &transform, alpha, &here, canvas, out)
            }
            _ => out.push(Placement {
                item,
                transform,
                opacity: alpha,
                rect: item_rect(&transform, canvas),
                path: here,
            }),
        }
    }
}

/// Turn a pixel crop against a known source size into the normalised crop the
/// document stores. A crop wider than the source is clamped rather than
/// refused: OBS files in the wild contain them.
pub fn normalise_crop(left: f64, top: f64, right: f64, bottom: f64, size: (f64, f64)) -> Crop {
    let clamp = |v: f64, of: f64| if of > 0.0 { (v / of).clamp(0.0, 1.0) } else { 0.0 };
    Crop {
        left: clamp(left, size.0),
        top: clamp(top, size.1),
        right: clamp(right, size.0),
        bottom: clamp(bottom, size.1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::document::{Align, Content, Item};

    fn canvas() -> Canvas {
        Canvas::default()
    }

    #[test]
    fn an_item_with_no_anchor_hangs_off_its_top_left_corner() {
        let t = Transform {
            position: Vec2::new(1400.0, 40.0),
            frame: Some(Frame::new(480.0, 270.0)),
            ..Transform::default()
        };
        assert_eq!(item_rect(&t, &canvas()), Rect::new(1400.0, 40.0, 480.0, 270.0));
    }

    #[test]
    fn a_centre_anchor_puts_the_middle_of_the_item_at_the_position() {
        let t = Transform {
            position: Vec2::new(960.0, 540.0),
            anchor: Vec2::new(0.5, 0.5),
            frame: Some(Frame::new(480.0, 270.0)),
            ..Transform::default()
        };
        assert_eq!(item_rect(&t, &canvas()), Rect::new(720.0, 405.0, 480.0, 270.0));
    }

    #[test]
    fn the_seven_fits_do_what_their_names_say() {
        let frame = Rect::new(0.0, 0.0, 1920.0, 1080.0);
        let content = (640.0, 480.0); // 4:3 into 16:9
        let cases = [
            (Fit::Stretch, (1920.0, 1080.0)),
            (Fit::Contain, (1440.0, 1080.0)),
            (Fit::Cover, (1920.0, 1440.0)),
            (Fit::FitWidth, (1920.0, 1440.0)),
            (Fit::FitHeight, (1440.0, 1080.0)),
            (Fit::Max, (640.0, 480.0)),
            (Fit::None, (640.0, 480.0)),
        ];
        for (fit, (w, h)) in cases {
            let r = fitted(frame, content, fit, Align::Center);
            assert!((r.w - w).abs() < EPSILON && (r.h - h).abs() < EPSILON, "{fit:?} gave {r:?}");
        }
    }

    #[test]
    fn alignment_moves_the_fitted_content_inside_its_frame() {
        let frame = Rect::new(0.0, 0.0, 1000.0, 1000.0);
        let left = fitted(frame, (100.0, 50.0), Fit::None, Align::TopLeft);
        let right = fitted(frame, (100.0, 50.0), Fit::None, Align::BottomRight);
        assert_eq!(left, Rect::new(0.0, 0.0, 100.0, 50.0));
        assert_eq!(right, Rect::new(900.0, 950.0, 100.0, 50.0));
    }

    #[test]
    fn a_group_multiplies_into_its_children() {
        let mut child = Item::new(Content::Source { source: "cam".into() });
        child.name = Some("child".into());
        child.transform.position = Vec2::new(100.0, 50.0);
        child.transform.frame = Some(Frame::new(200.0, 100.0));
        let mut group = Item::new(Content::Children { children: vec![child] });
        group.name = Some("group".into());
        group.transform.position = Vec2::new(1000.0, 500.0);
        group.transform.scale = Vec2::new(2.0, 2.0);
        group.opacity = 0.5;

        let items = [group];
        let placed = flatten(&items, &canvas());
        assert_eq!(placed.len(), 1, "the group itself is not a placement, its children are");
        assert_eq!(placed[0].rect, Rect::new(1200.0, 600.0, 400.0, 200.0));
        assert_eq!(placed[0].opacity, 0.5);
        assert_eq!(placed[0].path, "group / child");
    }

    #[test]
    fn a_rotated_group_carries_its_children_round_with_it() {
        let mut child = Item::new(Content::Source { source: "cam".into() });
        child.transform.position = Vec2::new(100.0, 0.0);
        child.transform.frame = Some(Frame::new(10.0, 10.0));
        let mut group = Item::new(Content::Children { children: vec![child] });
        group.transform.rotation = 90.0;
        let items = [group];
        let placed = flatten(&items, &canvas());
        assert!(placed[0].rect.x.abs() < EPSILON, "{:?}", placed[0].rect);
        assert!((placed[0].rect.y - 100.0).abs() < EPSILON, "{:?}", placed[0].rect);
        assert_eq!(placed[0].transform.rotation, 90.0);
    }

    #[test]
    fn an_invisible_item_is_not_placed_at_all() {
        let mut item = Item::new(Content::Source { source: "cam".into() });
        item.visible = false;
        let items = [item];
        assert!(flatten(&items, &canvas()).is_empty());
    }

    #[test]
    fn pixel_crops_become_fractions_of_the_source() {
        let c = normalise_crop(160.0, 0.0, 160.0, 90.0, (1920.0, 1080.0));
        assert!((c.left - 1.0 / 12.0).abs() < 1e-9);
        assert_eq!(c.top, 0.0);
        assert!((c.bottom - 1.0 / 12.0).abs() < 1e-9);
        // A crop wider than the source clamps rather than going past the edge.
        assert_eq!(normalise_crop(4000.0, 0.0, 0.0, 0.0, (1920.0, 1080.0)).left, 1.0);
    }

    #[test]
    fn rectangles_know_when_they_overlap_and_by_how_much() {
        let a = Rect::new(0.0, 0.0, 100.0, 100.0);
        let b = Rect::new(50.0, 50.0, 100.0, 100.0);
        assert_eq!(a.intersect(&b).unwrap().area(), 2500.0);
        assert!(a.intersect(&Rect::new(200.0, 0.0, 10.0, 10.0)).is_none());
        assert!(Rect::new(10.0, 10.0, 10.0, 10.0).inside(&a));
        assert!(!b.inside(&a));
    }
}
