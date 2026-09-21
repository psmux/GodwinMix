//! The canvas contract.
//!
//! Every source, ad and slate is converted to exactly these caps before it
//! reaches a mixer. Because all mixer inputs are then bit-identical in format,
//! switching between them is a property change rather than a renegotiation,
//! and the encoder downstream never sees a caps event after it starts.
//!
//! If you find yourself wanting a second set of raw caps somewhere, you have
//! probably found a bug.

use crate::config::Canvas;
use gstreamer as gst;

/// Raw video format used across the whole graph. I420 is what every H.264
/// encoder on every backend accepts without an extra conversion.
pub const VIDEO_FORMAT: &str = "I420";
/// Raw audio format. S16LE keeps the mixer cheap and matches every AAC encoder.
pub const AUDIO_FORMAT: &str = "S16LE";
/// Colour space of the canvas: BT.709, limited range, what every HD broadcast
/// chain and every RTMP player assumes. Sources in anything else are converted
/// on the way in.
pub const COLORIMETRY: &str = "bt709";

#[derive(Debug, Clone)]
pub struct CanvasCaps {
    pub width: i32,
    pub height: i32,
    pub fps: gst::Fraction,
    pub sample_rate: i32,
    pub channels: i32,
}

impl CanvasCaps {
    pub fn new(c: &Canvas) -> Self {
        Self {
            width: c.width,
            height: c.height,
            fps: gst::Fraction::new(c.fps, 1),
            sample_rate: c.sample_rate,
            channels: c.channels,
        }
    }

    /// Full canvas video caps, used for the program path.
    pub fn video(&self) -> gst::Caps {
        Self::video_at(self.width, self.height, self.fps)
    }

    /// Video caps at an arbitrary size, used for multiview cells. The format
    /// and pixel aspect ratio stay identical to the program path so that a
    /// downscaled branch never needs a converter it did not ask for.
    pub fn video_at(width: i32, height: i32, fps: gst::Fraction) -> gst::Caps {
        gst::Caps::builder("video/x-raw")
            .field("format", VIDEO_FORMAT)
            .field("width", width)
            .field("height", height)
            .field("framerate", fps)
            .field("pixel-aspect-ratio", gst::Fraction::new(1, 1))
            .field("interlace-mode", "progressive")
            // Pinned on purpose. Without it the compositor takes its output
            // colorimetry from whichever pad it likes best, and that changed
            // as sources came and went: one camera decoded with an unknown
            // range flipped the programme to "range unknown", GStreamer
            // treats unknown as full range, and every properly tagged source
            // after that was stretched from 16..235 to 0..255 on air.
            .field("colorimetry", COLORIMETRY)
            .build()
    }

    pub fn audio(&self) -> gst::Caps {
        gst::Caps::builder("audio/x-raw")
            .field("format", AUDIO_FORMAT)
            .field("rate", self.sample_rate)
            .field("channels", self.channels)
            .field("layout", "interleaved")
            .build()
    }

    /// Audio caps at an arbitrary format, rate and channel count, for a
    /// monitoring branch that a client asked to have downsized.
    pub fn audio_at(format: &str, rate: i32, channels: i32) -> gst::Caps {
        gst::Caps::builder("audio/x-raw")
            .field("format", format)
            .field("rate", rate)
            .field("channels", channels)
            .field("layout", "interleaved")
            .build()
    }

    /// Duration of a single video frame. Used for scheduling takes on frame
    /// boundaries and for sizing jitter buffers.
    #[allow(dead_code)]
    pub fn frame_duration(&self) -> gst::ClockTime {
        let n = self.fps.numer() as u64;
        let d = self.fps.denom() as u64;
        gst::ClockTime::from_nseconds(gst::ClockTime::SECOND.nseconds() * d / n.max(1))
    }
}

/// Grid geometry for the multiview mosaic.
///
/// Cells are laid out left to right, top to bottom, in a square-ish grid sized
/// to the number of tiles. Every cell keeps the source aspect ratio and is
/// letterboxed inside its rectangle, so a 4:3 camera next to a 16:9 one does
/// not get stretched.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Grid {
    pub cols: u32,
    pub rows: u32,
    pub cell_w: i32,
    pub cell_h: i32,
}

impl Grid {
    pub fn for_tiles(tiles: u32, width: i32, height: i32) -> Self {
        let tiles = tiles.max(1);
        let cols = (tiles as f64).sqrt().ceil() as u32;
        let rows = tiles.div_ceil(cols);
        // Round cell dimensions down to even numbers for 4:2:0 chroma.
        let cell_w = ((width / cols as i32) / 2) * 2;
        // A cell is the shape of the sheet, never taller. Dividing the height
        // by the rows gave two tiles, one source and the programme, a cell of
        // 480 by 540 each on a 960 by 540 sheet, and a client that fits a cell
        // into a 16:9 tile drew a wide camera as a narrow upright strip. The
        // rows that are not needed stay empty at the bottom of the sheet.
        let by_rows = height / rows as i32;
        let by_shape = (cell_w as i64 * height as i64 / width.max(1) as i64) as i32;
        let cell_h = (by_rows.min(by_shape) / 2) * 2;
        Self { cols, rows, cell_w, cell_h }
    }

    /// Top-left corner of cell `index`, counting from zero in reading order.
    pub fn cell_origin(&self, index: u32) -> (i32, i32) {
        let col = index % self.cols;
        let row = index / self.cols;
        (col as i32 * self.cell_w, row as i32 * self.cell_h)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_shapes() {
        assert_eq!(Grid::for_tiles(1, 1280, 720).cols, 1);
        assert_eq!(Grid::for_tiles(4, 1280, 720).cols, 2);
        assert_eq!(Grid::for_tiles(4, 1280, 720).rows, 2);
        assert_eq!(Grid::for_tiles(5, 1280, 720).cols, 3);
        assert_eq!(Grid::for_tiles(5, 1280, 720).rows, 2);
        assert_eq!(Grid::for_tiles(9, 1280, 720).cols, 3);
    }

    /// One source and the programme is two tiles, and two tiles used to be
    /// two cells taller than they were wide.
    #[test]
    fn a_cell_is_the_shape_of_the_sheet_however_many_tiles_there_are() {
        for tiles in 1..=12 {
            let g = Grid::for_tiles(tiles, 960, 540);
            let shape = g.cell_w as f64 / g.cell_h as f64;
            assert!((shape - 16.0 / 9.0).abs() < 0.03, "{tiles} tiles gave {} by {}", g.cell_w, g.cell_h);
            assert!(g.cell_h * g.rows as i32 <= 540, "{tiles} tiles overflow the sheet");
        }
        let two = Grid::for_tiles(2, 960, 540);
        assert_eq!((two.cell_w, two.cell_h), (480, 270));
    }

    #[test]
    fn cells_are_even_sized_and_fit_inside_the_mosaic() {
        for tiles in 1..=16u32 {
            let g = Grid::for_tiles(tiles, 1280, 720);
            assert_eq!(g.cell_w % 2, 0, "odd cell width for {tiles} tiles");
            assert_eq!(g.cell_h % 2, 0, "odd cell height for {tiles} tiles");
            for i in 0..tiles {
                let (x, y) = g.cell_origin(i);
                assert!(x + g.cell_w <= 1280, "cell {i} of {tiles} overflows width");
                assert!(y + g.cell_h <= 720, "cell {i} of {tiles} overflows height");
            }
        }
    }

    #[test]
    fn frame_duration_matches_framerate() {
        let c = CanvasCaps::new(&Canvas::default());
        assert_eq!(c.frame_duration().mseconds(), 33);
    }
}
