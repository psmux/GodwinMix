//! A rectangle of a page, which is all graphic mode deals in.

/// A rectangle of the page, in pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Area {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Area {
    pub fn is_empty(&self) -> bool {
        self.w <= 0 || self.h <= 0
    }

    pub fn union(self, o: Area) -> Area {
        if self.is_empty() {
            return o;
        }
        if o.is_empty() {
            return self;
        }
        let (x0, y0) = (self.x.min(o.x), self.y.min(o.y));
        let (x1, y1) = ((self.x + self.w).max(o.x + o.w), (self.y + self.h).max(o.y + o.h));
        Area { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }

    pub fn clamp(self, w: i32, h: i32) -> Area {
        let (x0, y0) = (self.x.clamp(0, w), self.y.clamp(0, h));
        let (x1, y1) = ((self.x + self.w).clamp(0, w), (self.y + self.h).clamp(0, h));
        Area { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}
