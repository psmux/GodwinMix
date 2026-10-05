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

use super::opaque::Opaque;
use super::wire::{apply, parse, picture, HEADER, PATCH, WHOLE};
use crate::overlay::carrier::Carrier;
use crate::overlay::picture::Area;
use crate::overlay::Layer;
use parking_lot::{Condvar, Mutex};
use std::io::Read;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

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

/// Where a renderer's pictures go: a transparent page to the layer (and its
/// carrier, for the tile), a page that covers the picture to `opaque`.
#[derive(Clone)]
pub struct Target {
    pub layer: Arc<Layer>,
    pub carrier: Arc<Carrier>,
    pub opaque: Option<Arc<Opaque>>,
}

/// Read frames from `pipe` until it closes, and keep the carrier current.
pub fn start(id: &str, pipe: Box<dyn Read + Send>, to: Target, feed: Arc<Feed>) {
    let (layer, carrier) = (to.layer.clone(), to.carrier.clone());
    let f = feed.clone();
    let reader = std::thread::Builder::new().name(format!("gmx-html-{id}")).spawn(move || {
        let why = read(pipe, &to, &f);
        tracing::debug!(reason = %why, "the HTML renderer's pictures stopped");
        f.end();
    });
    if let Err(e) = reader {
        tracing::warn!(error = %e, "could not start the HTML graphic reader");
        return;
    }
    let _ = std::thread::Builder::new().name(format!("gmx-html-carrier-{id}")).spawn(move || refresh(&layer, &carrier, &feed));
}

fn read(mut pipe: Box<dyn Read + Send>, to: &Target, feed: &Feed) -> String {
    let layer = &to.layer;
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
            if let Some(o) = &to.opaque {
                o.show(&data, area.w, area.h);
            }
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
