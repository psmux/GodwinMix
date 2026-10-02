//! One picture a transparent source hands the board, and how it moves.

use gstreamer as gst;

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
    /// The part of the natural frame this picture covers, in natural pixels,
    /// when it is not the whole of it: a keyed camera sends only what its
    /// matte keeps, and is drawn there rather than stretched to the box.
    pub within: Option<Area>,
    /// A keyed camera frame, drawn from the frame and its key rather than from
    /// `buffer`, which is then empty. See `overlay::keyed`.
    pub keyed: Option<std::sync::Arc<super::keyed::Keyed>>,
}

/// A rectangle of a picture's natural frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Area {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Picture {
    /// A picture from bytes already in AYUV, `width * 4` to a row.
    pub fn from_ayuv(data: Vec<u8>, width: u32, height: u32, natural: (u32, u32)) -> Picture {
        Picture { buffer: gst::Buffer::from_mut_slice(data), width, height, stride: width as usize * 4, natural, within: None, keyed: None }
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
