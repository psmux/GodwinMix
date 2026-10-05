//! The renderer's pictures, read off its stdout into the layer.
//!
//! The browser renderer in graphic mode writes a frame only when the page
//! painted, and only the box around what is not transparent, already in
//! AYUV (`browser/src/graphic.rs`):
//!
//! ```text
//!   "GMXF"  u32 LE: page width, page height, x, y, w, h   then w*h*4 bytes
//! ```
//!
//! Each becomes the layer's picture with `within` saying where on the page it
//! sits, so the board blends exactly that box and nothing else. A `GMXP`
//! frame is a patch: a part of the box that changed while the box did not,
//! copied into the picture held here and handed on as a new one. This thread
//! blocks on the pipe, never on the programme. The carrier, the grey frame
//! the tile and the supervisor see, is made again at most twice a second.

use crate::overlay::carrier::Carrier;
use crate::overlay::picture::{Area, Picture};
use crate::overlay::Layer;
use parking_lot::{Condvar, Mutex};
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const MAGIC: &[u8; 4] = b"GMXF";
const PATCH: &[u8; 4] = b"GMXP";
const WHOLE: &[u8; 4] = b"GMXI";
const HEADER: usize = 28;
/// The largest page the renderer is ever asked for, as a check on a header.
const MAX_SIDE: u32 = 8192;
/// How often the carrier may be made again.
const CARRIER_EVERY: Duration = Duration::from_millis(500);

/// What the reader and the carrier thread share.
pub struct Feed {
    /// Pictures read so far, for a test and for `call("frames")`.
    pub frames: AtomicU64,
    /// Set when the pipe closed: the renderer has gone.
    pub ended: AtomicBool,
    newer: Mutex<bool>,
    wake: Condvar,
}

impl Feed {
    pub fn new() -> Arc<Feed> {
        Arc::new(Feed { frames: AtomicU64::new(0), ended: AtomicBool::new(false), newer: Mutex::new(false), wake: Condvar::new() })
    }

    /// Stop the carrier thread.
    pub fn end(&self) {
        self.ended.store(true, Ordering::Release);
        self.wake.notify_all();
    }
}

/// Read frames from `pipe` until it closes, and keep the carrier current.
pub fn start(id: &str, pipe: Box<dyn Read + Send>, layer: Arc<Layer>, carrier: Arc<Carrier>, feed: Arc<Feed>) {
    let (l, f, c) = (layer.clone(), feed.clone(), carrier.clone());
    let reader = std::thread::Builder::new().name(format!("gmx-html-{id}")).spawn(move || {
        let why = read(pipe, &l, &c, &f);
        tracing::debug!(reason = %why, "the HTML renderer's pictures stopped");
        f.end();
    });
    if let Err(e) = reader {
        tracing::warn!(error = %e, "could not start the HTML graphic reader");
        return;
    }
    let _ = std::thread::Builder::new().name(format!("gmx-html-carrier-{id}")).spawn(move || refresh(&layer, &carrier, &feed));
}

fn read(mut pipe: Box<dyn Read + Send>, layer: &Layer, carrier: &Carrier, feed: &Feed) -> String {
    let mut header = [0u8; HEADER];
    // The whole box last sent, kept to copy patches into.
    let mut held: Option<(Area, Vec<u8>)> = None;
    loop {
        if let Err(e) = pipe.read_exact(&mut header) {
            return e.to_string();
        }
        let Some((page, area)) = parse(&header) else { return "a frame that is not one".into() };
        let whole_frame = &header[..4] == WHOLE;
        let bytes = if whole_frame { area.w as usize * area.h as usize * 3 / 2 } else { area.w as usize * area.h as usize * 4 };
        let mut data = vec![0u8; bytes];
        if let Err(e) = pipe.read_exact(&mut data) {
            return e.to_string();
        }
        if whole_frame {
            // A design that covers the picture: straight to the compositor.
            carrier.show_i420(&data, area.w, area.h);
            feed.frames.fetch_add(1, Ordering::Relaxed);
            continue;
        }
        let whole = if &header[..4] == PATCH {
            let Some((at, pixels)) = held.as_mut() else { continue };
            if !apply(pixels, *at, &data, area) {
                continue;
            }
            (*at, pixels.clone())
        } else {
            held = Some((area, data.clone()));
            (area, data)
        };
        layer.set_picture(picture(whole.1, page, whole.0).map(Arc::new));
        feed.frames.fetch_add(1, Ordering::Relaxed);
        *feed.newer.lock() = true;
        feed.wake.notify_one();
    }
}

/// The page size and the box a header describes.
pub fn parse(h: &[u8; HEADER]) -> Option<((u32, u32), Area)> {
    if &h[..4] != MAGIC && &h[..4] != PATCH && &h[..4] != WHOLE {
        return None;
    }
    let n = |i: usize| u32::from_le_bytes([h[4 + i * 4], h[5 + i * 4], h[6 + i * 4], h[7 + i * 4]]);
    let (pw, ph, x, y, w, h) = (n(0), n(1), n(2), n(3), n(4), n(5));
    let fits = pw <= MAX_SIDE && ph <= MAX_SIDE && x.saturating_add(w) <= pw && y.saturating_add(h) <= ph;
    fits.then_some(((pw, ph), Area { x, y, w, h }))
}

/// Copy a patch covering `part` of the page into `pixels`, which hold the
/// box `at`. False when the patch is not inside the box.
pub fn apply(pixels: &mut [u8], at: Area, patch: &[u8], part: Area) -> bool {
    let fits = part.x >= at.x && part.y >= at.y && part.x + part.w <= at.x + at.w && part.y + part.h <= at.y + at.h;
    if !fits {
        return false;
    }
    let (row, stride) = (part.w as usize * 4, at.w as usize * 4);
    for y in 0..part.h as usize {
        let to = (part.y - at.y) as usize * stride + y * stride + (part.x - at.x) as usize * 4;
        pixels[to..to + row].copy_from_slice(&patch[y * row..(y + 1) * row]);
    }
    true
}

/// A box of AYUV as a picture placed on its page. None for an empty page.
pub fn picture(data: Vec<u8>, page: (u32, u32), area: Area) -> Option<Picture> {
    if area.w == 0 || area.h == 0 {
        return None;
    }
    Some(Picture { within: Some(area), ..Picture::from_ayuv(data, area.w, area.h, page) })
}

/// Make the carrier again when a newer picture has come, at most every
/// `CARRIER_EVERY`, until the feed ends.
fn refresh(layer: &Layer, carrier: &Carrier, feed: &Feed) {
    while !feed.ended.load(Ordering::Acquire) {
        {
            let mut newer = feed.newer.lock();
            while !*newer && !feed.ended.load(Ordering::Acquire) {
                feed.wake.wait(&mut newer);
            }
            *newer = false;
        }
        carrier.show(layer.picture().as_deref());
        std::thread::sleep(CARRIER_EVERY);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_reads_back_and_a_bad_one_is_refused() {
        let mut h = [0u8; HEADER];
        h[..4].copy_from_slice(MAGIC);
        for (i, v) in [1920u32, 1080, 100, 800, 900, 200].iter().enumerate() {
            h[4 + i * 4..8 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(parse(&h), Some(((1920, 1080), Area { x: 100, y: 800, w: 900, h: 200 })));
        h[20..24].copy_from_slice(&2000u32.to_le_bytes());
        assert_eq!(parse(&h), None, "a box off the page");
        h[0] = b'X';
        assert_eq!(parse(&h), None);
    }

    #[test]
    fn a_patch_lands_inside_the_box_it_belongs_to() {
        let at = Area { x: 10, y: 10, w: 4, h: 4 };
        let mut pixels = vec![0u8; 64];
        assert!(apply(&mut pixels, at, &[9u8; 8], Area { x: 11, y: 12, w: 1, h: 2 }));
        assert_eq!(&pixels[2 * 16 + 4..2 * 16 + 8], &[9, 9, 9, 9]);
        assert_eq!(&pixels[3 * 16 + 4..3 * 16 + 8], &[9, 9, 9, 9]);
        assert_eq!(pixels.iter().filter(|b| **b == 9).count(), 8);
        assert!(!apply(&mut pixels, at, &[9u8; 4], Area { x: 20, y: 12, w: 1, h: 1 }));
    }

    #[test]
    fn an_empty_box_is_no_picture_and_a_box_is_placed_on_its_page() {
        assert!(picture(Vec::new(), (1920, 1080), Area { x: 0, y: 0, w: 0, h: 0 }).is_none());
        let p = picture(vec![255; 16], (1920, 1080), Area { x: 10, y: 20, w: 2, h: 2 }).unwrap();
        assert_eq!((p.natural, p.within, p.width), ((1920, 1080), Some(Area { x: 10, y: 20, w: 2, h: 2 }), 2));
    }
}
