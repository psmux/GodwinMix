//! Graphic mode (`--graphic`): a transparent page as pictures, not a stream.
//!
//! A lower third, a bug or a ticker drawn by a web page is mostly nothing.
//! Sent as a stream it costs a camera: every frame, the whole canvas,
//! converted and muxed whether or not a pixel moved (a blank transparent
//! page took 125 percent of a core at 1080p30 on the development laptop).
//! Here a frame leaves only when Chromium painted one, and only the box
//! around what is not fully transparent, in the AYUV the overlay board
//! draws (A, Y, U, V, straight alpha, BT.709 limited range):
//!
//! ```text
//!   "GMXF"  u32 LE: page width, page height, x, y, w, h   then w*h*4 bytes
//!   "GMXP"  the same, a patch: a change inside the last box, away from its edges
//!   "GMXI"  u32 LE: width, height, 0, 0, width, height   then w*h*3/2 bytes of I420
//! ```
//!
//! `w` and `h` are zero when the page is empty. `GMXI` is the whole page,
//! for a design that covers the picture (`--opaque`), which needs no alpha
//! and goes to the compositor like a camera.

pub use crate::area::Area;
use crate::pixels::{content, copy_area, encode, encode_as, inside};
use std::io::Write;
use std::sync::{Arc, Condvar, Mutex};

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
