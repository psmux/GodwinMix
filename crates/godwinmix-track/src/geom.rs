//! Rectangles and points in normalised coordinates: 0 is the left or top edge
//! of the frame and 1 the right or bottom, whatever its size in pixels.

/// A point in normalised frame coordinates.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// Distance to `other`, in normalised units.
    pub fn distance(self, other: Point) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// An axis aligned rectangle: the top left corner and the size, normalised.
/// A rectangle may reach outside 0..1; a PTZ target often does.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    /// The whole frame.
    pub const FULL: Rect = Rect { x: 0.0, y: 0.0, w: 1.0, h: 1.0 };

    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    /// A rectangle of size `w` by `h` centred on `(cx, cy)`.
    pub fn from_center(cx: f32, cy: f32, w: f32, h: f32) -> Self {
        Self { x: cx - w / 2.0, y: cy - h / 2.0, w, h }
    }

    /// A rectangle from pixel coordinates in a frame of `width` by `height`.
    pub fn from_pixels(x: f32, y: f32, w: f32, h: f32, width: u32, height: u32) -> Self {
        let (fw, fh) = (width.max(1) as f32, height.max(1) as f32);
        Self { x: x / fw, y: y / fh, w: w / fw, h: h / fh }
    }

    pub fn cx(&self) -> f32 {
        self.x + self.w / 2.0
    }

    pub fn cy(&self) -> f32 {
        self.y + self.h / 2.0
    }

    pub fn center(&self) -> Point {
        Point::new(self.cx(), self.cy())
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn area(&self) -> f32 {
        self.w.max(0.0) * self.h.max(0.0)
    }

    /// The overlap of two rectangles, empty (zero size) when they do not meet.
    pub fn intersection(&self, o: &Rect) -> Rect {
        let x = self.x.max(o.x);
        let y = self.y.max(o.y);
        let r = self.right().min(o.right());
        let b = self.bottom().min(o.bottom());
        Rect::new(x, y, (r - x).max(0.0), (b - y).max(0.0))
    }

    /// Intersection over union, 0 for disjoint rectangles and 1 for equal ones.
    pub fn iou(&self, o: &Rect) -> f32 {
        let inter = self.intersection(o).area();
        let union = self.area() + o.area() - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.right() && p.y >= self.y && p.y <= self.bottom()
    }

    /// The same rectangle moved, and if need be shrunk about its centre, so
    /// that it lies inside the frame. Shrinking keeps the aspect ratio.
    pub fn clamp_inside(&self) -> Rect {
        let scale = (1.0 / self.w.max(1e-6)).min(1.0 / self.h.max(1e-6)).min(1.0);
        let (w, h) = (self.w * scale, self.h * scale);
        let x = (self.cx() - w / 2.0).clamp(0.0, 1.0 - w);
        let y = (self.cy() - h / 2.0).clamp(0.0, 1.0 - h);
        Rect::new(x, y, w, h)
    }

    /// True when every field is a finite number.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite() && self.w.is_finite() && self.h.is_finite()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iou_of_equal_disjoint_and_half_overlapping_rects() {
        let a = Rect::new(0.0, 0.0, 0.2, 0.2);
        assert!((a.iou(&a) - 1.0).abs() < 1e-6);
        assert_eq!(a.iou(&Rect::new(0.5, 0.5, 0.1, 0.1)), 0.0);
        let b = Rect::new(0.1, 0.0, 0.2, 0.2);
        assert!((a.iou(&b) - 1.0 / 3.0).abs() < 1e-5);
    }

    #[test]
    fn clamp_inside_moves_then_shrinks_keeping_aspect() {
        let r = Rect::new(0.9, -0.1, 0.2, 0.3).clamp_inside();
        assert!((r.x - 0.8).abs() < 1e-6 && r.y == 0.0);
        let big = Rect::from_center(0.5, 0.5, 2.0, 1.0).clamp_inside();
        assert!((big.w - 1.0).abs() < 1e-6 && (big.h - 0.5).abs() < 1e-6);
    }
}
