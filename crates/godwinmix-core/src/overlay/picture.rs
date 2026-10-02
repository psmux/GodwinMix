//! What a transparent source hands the board: one picture, how it moves, and
//! the size it is being drawn at.

use gstreamer as gst;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// One rendered picture in AYUV: four bytes a pixel, alpha first, straight
/// (not premultiplied) alpha, BT.709 limited range like the canvas.
///
/// Held as a buffer so a decoded video frame is drawn from the decoder's own
/// memory with no copy, and a rendered text from a buffer made once.
pub struct Picture {
    pub buffer: gst::Buffer,
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    /// The size the source has at a scale of one: an image's own pixels, an
    /// SVG's declared size, a text's laid out box. What `contain` and `cover`
    /// keep the shape of, whatever size this one copy happens to be.
    pub natural: (u32, u32),
}

impl Picture {
    /// A picture from bytes already in AYUV, `width * 4` to a row.
    pub fn from_ayuv(data: Vec<u8>, width: u32, height: u32, natural: (u32, u32)) -> Picture {
        Picture { buffer: gst::Buffer::from_mut_slice(data), width, height, stride: width as usize * 4, natural }
    }
}

/// Which way a ticker moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Right to left, the usual news crawl.
    Left,
    /// Left to right.
    Right,
    /// Bottom to top, for credits.
    Up,
}

/// How the board draws a layer's picture inside the item's box.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Motion {
    /// Fitted into the box the way the item's `fit` says, and held.
    Still,
    /// Moved through the box at `speed` pixels a second. With `repeat` the
    /// strip follows itself round with `gap` pixels between; without, it
    /// crosses once and the box stays empty after.
    Crawl { speed: f64, direction: Direction, gap: u32, repeat: bool },
}

type Resized = Box<dyn Fn((u32, u32)) + Send + Sync>;

/// What one transparent source shares with the board.
///
/// The kind writes the picture and the motion; the board reads them once a
/// programme frame and writes back the size the picture is being drawn at, so
/// a text or an SVG can be rendered again at that size rather than scaled.
pub struct Layer {
    picture: Mutex<Option<Arc<Picture>>>,
    motion: Mutex<Motion>,
    /// Whether the board draws this source. A clip is only known to carry
    /// alpha once its decoder says so, and one that turns out opaque goes
    /// through the compositor like any other source.
    active: AtomicBool,
    drawn: Mutex<Option<(u32, u32)>>,
    resized: Mutex<Option<Resized>>,
    /// Bumped when the words change, so a crawl starts again from the edge
    /// rather than carrying on from where the old words had got to.
    epoch: AtomicU64,
}

impl Layer {
    pub fn new(active: bool) -> Arc<Layer> {
        Arc::new(Layer {
            picture: Mutex::new(None),
            motion: Mutex::new(Motion::Still),
            active: AtomicBool::new(active),
            drawn: Mutex::new(None),
            resized: Mutex::new(None),
            epoch: AtomicU64::new(0),
        })
    }

    pub fn set_picture(&self, picture: Option<Arc<Picture>>) {
        *self.picture.lock() = picture;
    }

    pub fn picture(&self) -> Option<Arc<Picture>> {
        self.picture.lock().clone()
    }

    pub fn set_motion(&self, motion: Motion) {
        *self.motion.lock() = motion;
    }

    pub fn motion(&self) -> Motion {
        *self.motion.lock()
    }

    /// Start a crawl again from the edge of its box.
    pub fn restart(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    pub fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub fn activate(&self, on: bool) {
        self.active.store(on, Ordering::Release);
    }

    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Who to tell when the drawn size changes. Called on the programme's
    /// streaming thread, so it must only hand the size on, never render.
    pub fn on_resize(&self, f: impl Fn((u32, u32)) + Send + Sync + 'static) {
        *self.resized.lock() = Some(Box::new(f));
    }

    /// The size this layer was last drawn at, if it has been drawn.
    pub fn drawn(&self) -> Option<(u32, u32)> {
        *self.drawn.lock()
    }

    /// Called by the board with the largest size the picture is drawn at
    /// this frame. Says so to the kind only when it changed.
    pub fn note_drawn(&self, size: (u32, u32)) {
        let mut drawn = self.drawn.lock();
        if *drawn == Some(size) {
            return;
        }
        *drawn = Some(size);
        drop(drawn);
        if let Some(f) = self.resized.lock().as_ref() {
            f(size);
        }
    }
}
