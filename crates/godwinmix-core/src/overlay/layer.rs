//! What one transparent source shares with the board: its picture, how it
//! moves, and the size it is being drawn at.

use super::picture::{Motion, Picture};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

type Resized = Box<dyn Fn((u32, u32)) + Send + Sync>;

/// What one transparent source shares with the board.
///
/// The kind writes the picture and the motion; the board reads them once a
/// programme frame and writes back the size the picture is being drawn at, so
/// a text or an SVG can be rendered again at that size rather than scaled.
pub struct Layer {
    picture: Mutex<Option<Arc<Picture>>>,
    /// Drawn still under a crawl, filling the box: a ticker's bar, which
    /// stays where it is while the words move across it.
    backdrop: Mutex<Option<Arc<Picture>>>,
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
            backdrop: Mutex::new(None),
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

    pub fn set_backdrop(&self, picture: Option<Arc<Picture>>) {
        *self.backdrop.lock() = picture;
    }

    pub fn backdrop(&self) -> Option<Arc<Picture>> {
        self.backdrop.lock().clone()
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
