//! How wide the programme thumbnail branch scales to.
//!
//! The wall asks for 160 and 320, and the branch scales to 320 so that fifty
//! shows on a small machine convert a small picture once a second. A snapshot
//! of the programme asks for more (an agent reading a lower third wants 1280),
//! and used to get the 320 cell cut out of the mosaic whatever it asked for.
//! So the branch's caps follow the widest picture asked for while it runs, up
//! to the canvas, and drop back to 320 when it is taken off the tee.

use super::thumb::BRANCH_WIDTH;
use gstreamer as gst;
use gstreamer::prelude::*;
use parking_lot::Mutex;

/// The capsfilter at the end of the branch, and the canvas it scales from.
#[derive(Default)]
pub struct Size {
    held: Mutex<Option<Held>>,
}

struct Held {
    filter: gst::Element,
    canvas: (i32, i32),
    width: u32,
}

impl Size {
    /// The branch was built with `filter` at [`BRANCH_WIDTH`].
    pub fn hold(&self, filter: gst::Element, canvas: (i32, i32)) {
        *self.held.lock() = Some(Held { filter, canvas, width: BRANCH_WIDTH });
    }

    /// The widest the branch can give: the canvas, or 320 with no branch.
    pub fn ceiling(&self) -> u32 {
        self.held.lock().as_ref().map_or(BRANCH_WIDTH, |h| (h.canvas.0.max(2) as u32) & !1)
    }

    /// Scale to at least `width` from the next frame. Answers the width the
    /// branch now scales to. Setting the caps from here is a property on an
    /// element, which the streaming thread picks up as a renegotiation.
    pub fn widen(&self, width: u32) -> u32 {
        let mut held = self.held.lock();
        let Some(h) = held.as_mut() else { return BRANCH_WIDTH };
        let width = width.clamp(BRANCH_WIDTH, h.canvas.0.max(2) as u32) & !1;
        if width > h.width {
            h.filter.set_property("caps", caps(width, h.canvas));
            h.width = width;
        }
        h.width
    }

    /// Back to 320, for the next time the branch goes on.
    pub fn reset(&self) {
        if let Some(h) = self.held.lock().as_mut().filter(|h| h.width != BRANCH_WIDTH) {
            h.filter.set_property("caps", caps(BRANCH_WIDTH, h.canvas));
            h.width = BRANCH_WIDTH;
        }
    }
}

/// RGB at `width`, the canvas's shape, square pixels.
pub fn caps(width: u32, canvas: (i32, i32)) -> gst::Caps {
    let height = (i64::from(width) * i64::from(canvas.1.max(2)) / i64::from(canvas.0.max(2))).max(2) as i32 & !1;
    gst::Caps::builder("video/x-raw")
        .field("format", "RGB")
        .field("width", width as i32)
        .field("height", height)
        .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_branch_widens_to_an_ask_up_to_the_canvas_and_resets() {
        gst::init().unwrap();
        let filter = gst::ElementFactory::make("capsfilter").build().unwrap();
        let size = Size::default();
        assert_eq!(size.widen(1280), BRANCH_WIDTH, "no branch, no widening");
        size.hold(filter.clone(), (1920, 1080));
        assert_eq!(size.ceiling(), 1920);
        assert_eq!(size.widen(160), BRANCH_WIDTH, "never narrower than the wall's 320");
        assert_eq!(size.widen(1281), 1280);
        assert_eq!(size.widen(4000), 1920);
        let set = filter.property::<gst::Caps>("caps");
        assert_eq!(set.structure(0).unwrap().get::<i32>("height").unwrap(), 1080);
        size.reset();
        let set = filter.property::<gst::Caps>("caps");
        assert_eq!(set.structure(0).unwrap().get::<i32>("width").unwrap(), 320);
    }
}
