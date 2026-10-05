//! Graphic mode (`--graphic`): a transparent page as pictures, not a stream.
//!
//! A lower third, a bug or a ticker drawn by a web page is mostly nothing.
//! Sent as a stream it costs what a camera costs: every frame at the canvas
//! rate, the whole canvas, converted and muxed whether or not a pixel moved.
//! Measured on a 2024 laptop at 1080p30, a blank transparent page sent that
//! way took 125 percent of a core.
//!
//! So in graphic mode nothing is paced and nothing is muxed. A frame leaves
//! only when Chromium painted one, and only the part of the page that has
//! anything in it leaves: the box around every pixel that is not fully
//! transparent, already in the AYUV the mixer's overlay board draws. A held
//! graphic sends nothing at all. On stdout, per frame:
//!
//! ```text
//!   "GMXF"  u32 LE: page width, page height, x, y, w, h   then w*h*4 bytes AYUV
//! ```
//!
//! `w` and `h` are zero when the page is empty. AYUV is A, Y, U, V per pixel,
//! straight alpha, BT.709 limited range, which is the canvas's colorimetry.
//!
//! When what changed lies inside the box already sent, away from its edges,
//! the box cannot have changed, and only the changed part is sent, as a
//! patch the mixer copies into the picture it holds:
//!
//! ```text
//!   "GMXP"  u32 LE: page width, page height, x, y, w, h   then w*h*4 bytes
//! ```
//!
//! That is what keeps a full screen design with one moving corner (a title
//! card with a spinning logo) costing the corner and not the screen.

use std::io::Write;
use std::sync::{Arc, Condvar, Mutex};

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

    fn clamp(self, w: i32, h: i32) -> Area {
        let (x0, y0) = (self.x.clamp(0, w), self.y.clamp(0, h));
        let (x1, y1) = ((self.x + self.w).clamp(0, w), (self.y + self.h).clamp(0, h));
        Area { x: x0, y: y0, w: x1 - x0, h: y1 - y0 }
    }
}

/// What the browser painted and the writer has not sent yet.
struct Pending {
    /// The whole page as Chromium last painted it, premultiplied BGRA.
    mirror: Vec<u8>,
    width: i32,
    height: i32,
    /// Everything painted since the last frame left.
    dirty: Area,
    stop: bool,
}

pub struct Out {
    pending: Mutex<Pending>,
    wake: Condvar,
}

impl Out {
    /// Start the writer thread. `on_fail` runs once if stdout goes away.
    pub fn start(width: i32, height: i32, on_fail: impl FnOnce() + Send + 'static) -> Arc<Out> {
        let mirror = vec![0u8; (width * height * 4).max(0) as usize];
        let out = Arc::new(Out {
            pending: Mutex::new(Pending { mirror, width, height, dirty: Area::default(), stop: false }),
            wake: Condvar::new(),
        });
        let writer = out.clone();
        std::thread::Builder::new()
            .name("graphic-out".into())
            .spawn(move || {
                if let Err(e) = writer.run(std::io::stdout().lock()) {
                    eprintln!("[browser] output stopped: {e}");
                }
                on_fail();
            })
            .expect("spawning the graphic writer");
        out
    }

    /// The browser painted `dirty` of a `width` by `height` page.
    pub fn paint(&self, bgra: &[u8], width: i32, height: i32, dirty: &[Area]) {
        let mut p = self.pending.lock().unwrap();
        if bgra.len() < (width * height * 4).max(0) as usize {
            return;
        }
        // Scaled, Chromium may paint a pixel more or less than was asked for:
        // take the size it painted, and the whole of it.
        if width != p.width || height != p.height {
            (p.width, p.height) = (width, height);
            p.mirror = vec![0u8; (width * height * 4).max(0) as usize];
            p.dirty = Area { x: 0, y: 0, w: width, h: height };
            let all = [p.dirty];
            drop(p);
            return self.paint(bgra, width, height, &all);
        }
        let stride = width as usize * 4;
        for d in dirty.iter().map(|d| d.clamp(width, height)).filter(|d| !d.is_empty()) {
            for y in d.y..d.y + d.h {
                let at = y as usize * stride + d.x as usize * 4;
                let len = d.w as usize * 4;
                p.mirror[at..at + len].copy_from_slice(&bgra[at..at + len]);
            }
            p.dirty = p.dirty.union(d);
        }
        self.wake.notify_one();
    }

    pub fn stop(&self) {
        self.pending.lock().unwrap().stop = true;
        self.wake.notify_one();
    }

    fn run(&self, mut to: impl Write) -> std::io::Result<()> {
        // The box last sent: everything outside it was empty then.
        let mut shown = Area::default();
        let mut region = Vec::new();
        loop {
            let Some((dirty, page)) = self.next_dirty() else { return Ok(()) };
            let frame = if inside(dirty, shown) {
                self.copy(dirty, &mut region);
                encode_as(b"GMXP", &region, dirty, dirty, page)
            } else {
                // Outside what was sent last time and what was painted since,
                // the page was empty and still is.
                let area = shown.union(dirty).clamp(page.0, page.1);
                self.copy(area, &mut region);
                shown = content(&region, area);
                encode(&region, area, shown, page)
            };
            to.write_all(&frame)?;
            to.flush()?;
        }
    }

    /// Wait for a paint and take what it changed, or None when stopping.
    fn next_dirty(&self) -> Option<(Area, (i32, i32))> {
        let mut p = self.pending.lock().unwrap();
        while p.dirty.is_empty() && !p.stop {
            p = self.wake.wait(p).unwrap();
        }
        if p.stop {
            return None;
        }
        let dirty = p.dirty.clamp(p.width, p.height);
        p.dirty = Area::default();
        Some((dirty, (p.width, p.height)))
    }

    /// Copy `area` of the page as it is now out into `into`.
    fn copy(&self, area: Area, into: &mut Vec<u8>) {
        let p = self.pending.lock().unwrap();
        copy_area(&p.mirror, p.width, area, into);
    }
}

/// Copy `area` of a page `width` wide out into `into`, `area.w * 4` a row.
fn copy_area(page: &[u8], width: i32, area: Area, into: &mut Vec<u8>) {
    into.clear();
    let stride = width as usize * 4;
    for y in area.y..area.y + area.h {
        let at = y as usize * stride + area.x as usize * 4;
        into.extend_from_slice(&page[at..at + area.w as usize * 4]);
    }
}

/// The box around every pixel of `region` with any alpha, in page pixels.
pub fn content(region: &[u8], area: Area) -> Area {
    let row = area.w.max(0) as usize * 4;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
    for (y, line) in region.chunks_exact(row.max(4)).enumerate().take(area.h.max(0) as usize) {
        let Some(first) = line.chunks_exact(4).position(|px| px[3] != 0) else { continue };
        let last = line.chunks_exact(4).rposition(|px| px[3] != 0).unwrap_or(first);
        x0 = x0.min(first as i32);
        x1 = x1.max(last as i32 + 1);
        y0 = y0.min(y as i32);
        y1 = y as i32 + 1;
    }
    if x1 < 0 {
        return Area::default();
    }
    Area { x: area.x + x0, y: area.y + y0, w: x1 - x0, h: y1 - y0 }
}

/// Whether `a` lies inside `b` without touching its edges, so the box
/// around the page's content is the same after `a` changed.
pub fn inside(a: Area, b: Area) -> bool {
    !a.is_empty() && !b.is_empty() && a.x > b.x && a.y > b.y && a.x + a.w < b.x + b.w && a.y + a.h < b.y + b.h
}

/// One frame on the wire: the header and `tight` of `region` (which covers
/// `area`) in AYUV.
pub fn encode(region: &[u8], area: Area, tight: Area, page: (i32, i32)) -> Vec<u8> {
    encode_as(b"GMXF", region, area, tight, page)
}

fn encode_as(magic: &[u8; 4], region: &[u8], area: Area, tight: Area, page: (i32, i32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + (tight.w.max(0) * tight.h.max(0) * 4) as usize);
    out.extend_from_slice(magic);
    for v in [page.0, page.1, tight.x, tight.y, tight.w.max(0), tight.h.max(0)] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    let stride = area.w as usize * 4;
    for y in tight.y..tight.y + tight.h {
        let at = (y - area.y) as usize * stride + (tight.x - area.x) as usize * 4;
        for px in region[at..at + tight.w as usize * 4].chunks_exact(4) {
            out.extend_from_slice(&ayuv(px));
        }
    }
    out
}

/// One premultiplied BGRA pixel as straight alpha AYUV, BT.709 limited range.
pub fn ayuv(px: &[u8]) -> [u8; 4] {
    let a = px[3] as i32;
    if a == 0 {
        return [0, 16, 128, 128];
    }
    let un = |c: u8| if a == 255 { c as i32 } else { (c as i32 * 255 / a).min(255) };
    let (b, g, r) = (un(px[0]), un(px[1]), un(px[2]));
    // BT.709 in 8 bit fixed point, scaled by 256.
    let y = 16 + ((47 * r + 157 * g + 16 * b + 128) >> 8);
    let u = 128 + ((-26 * r - 86 * g + 112 * b + 128) >> 8);
    let v = 128 + ((112 * r - 102 * g - 10 * b + 128) >> 8);
    [a as u8, y.clamp(16, 235) as u8, u.clamp(16, 240) as u8, v.clamp(16, 240) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_black_and_clear_convert_to_the_canvas_levels() {
        assert_eq!(ayuv(&[255, 255, 255, 255]), [255, 235, 128, 128]);
        assert_eq!(ayuv(&[0, 0, 0, 255]), [255, 16, 128, 128]);
        assert_eq!(ayuv(&[0, 0, 0, 0]), [0, 16, 128, 128]);
        // Half covered white, premultiplied, is still white at half alpha.
        assert_eq!(ayuv(&[128, 128, 128, 128])[1], 235);
    }

    #[test]
    fn only_the_painted_box_is_sent() {
        let area = Area { x: 10, y: 20, w: 4, h: 3 };
        let mut region = vec![0u8; 4 * 3 * 4];
        region[(4 + 2) * 4 + 3] = 255; // row 1, column 2
        let tight = content(&region, area);
        assert_eq!(tight, Area { x: 12, y: 21, w: 1, h: 1 });
        let frame = encode(&region, area, tight, (100, 50));
        assert_eq!(&frame[..4], b"GMXF");
        assert_eq!(frame.len(), 28 + 4);
        assert_eq!(content(&vec![0u8; 48], area), Area::default());
    }
}
