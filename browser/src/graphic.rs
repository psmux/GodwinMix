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
//!
//! A design that covers the whole picture (`--opaque`: a background, a
//! title card) has no alpha to keep and goes to the compositor like a
//! camera, so it is sent whole, as I420, a third of the bytes of AYUV:
//!
//! ```text
//!   "GMXI"  u32 LE: page width, page height, 0, 0, width, height   then w*h*3/2 bytes
//! ```

use crate::pixels::{content, copy_area, encode, encode_as, inside};
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
    /// The whole page as I420 every time, for a design that covers the
    /// picture (`--opaque`).
    opaque: bool,
}

impl Out {
    /// Start the writer thread. `on_fail` runs once if stdout goes away.
    pub fn start(width: i32, height: i32, opaque: bool, on_fail: impl FnOnce() + Send + 'static) -> Arc<Out> {
        let mirror = vec![0u8; (width * height * 4).max(0) as usize];
        let out = Arc::new(Out {
            pending: Mutex::new(Pending { mirror, width, height, dirty: Area::default(), stop: false }),
            wake: Condvar::new(),
            opaque,
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
            let frame = if self.opaque {
                let p = self.pending.lock().unwrap();
                crate::pixels::encode_i420(&p.mirror, p.width, p.height)
            } else if inside(dirty, shown) {
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
