//! One picture a transparent source hands the board, and how it moves.

use super::blend::Rect;
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
    /// The part of the picture that is not fully transparent, when the kind
    /// measured it. The board draws only this part, so a lower third laid
    /// out on a whole canvas costs the blend of its panel, not of the frame.
    pub content: Option<Rect>,
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
        Picture { buffer: gst::Buffer::from_mut_slice(data), width, height, stride: width as usize * 4, natural, content: None, within: None, keyed: None }
    }

    /// The window of this picture to draw and where it lands, when the whole
    /// of it is drawn on `to`: the content box only, when it was measured.
    /// Rounded outwards by a pixel, so an edge is never cut.
    pub fn drawn_part(&self, to: Rect) -> (Rect, Rect) {
        let Some(part) = self.content else { return (Rect::new(0, 0, self.width as i32, self.height as i32), to) };
        let sx = to.w as f64 / self.width.max(1) as f64;
        let sy = to.h as f64 / self.height.max(1) as f64;
        let x0 = (to.x as f64 + part.x as f64 * sx).floor() as i32;
        let y0 = (to.y as f64 + part.y as f64 * sy).floor() as i32;
        let x1 = (to.x as f64 + part.right() as f64 * sx).ceil() as i32;
        let y1 = (to.y as f64 + part.bottom() as f64 * sy).ceil() as i32;
        (part, Rect::new(x0, y0, (x1 - x0).max(1), (y1 - y0).max(1)))
    }

    /// The same picture with `content` measured: one pass over its alpha,
    /// once per render. A picture with nothing visible keeps a one pixel
    /// content box, so it still draws (nothing) rather than the whole frame.
    pub fn measured(mut self) -> Picture {
        let Ok(map) = self.buffer.map_readable() else { return self };
        let (w, h) = (self.width as usize, self.height as usize);
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            let row = &map[y * self.stride..y * self.stride + w * 4];
            let Some(first) = row.chunks_exact(4).position(|px| px[0] != 0) else { continue };
            let last = row.chunks_exact(4).rposition(|px| px[0] != 0).unwrap_or(first);
            (x0, x1, y0, y1) = (x0.min(first), x1.max(last + 1), y0.min(y), y + 1);
        }
        drop(map);
        self.content = Some(if x1 > x0 { Rect::new(x0 as i32, y0 as i32, (x1 - x0) as i32, (y1 - y0) as i32) } else { Rect::new(0, 0, 1, 1) });
        self
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
