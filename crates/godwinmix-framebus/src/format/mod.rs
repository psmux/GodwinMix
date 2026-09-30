//! Pixel formats and how a frame's planes sit inside a slot.
//!
//! A slot holds one frame as the planes GStreamer would hand a sink, each row
//! padded to a multiple of [`ROW_ALIGN`] bytes so SIMD code downstream can read
//! whole vectors. The layout travels in the region header, so a reader never
//! guesses it.

use crate::Error;

/// Rows start on this many bytes. 64 is a cache line on every target we ship.
pub const ROW_ALIGN: usize = 64;

/// The formats the bus carries. NV12 and I420 are what decoders emit; the rest
/// are what capture cards, screen grabs and 10 bit sources emit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum Format {
    Nv12 = 1,
    I420 = 2,
    /// NV12 with 16 bit samples holding 10 bits, as HEVC Main 10 decodes.
    P010 = 3,
    Yuy2 = 4,
    Uyvy = 5,
    Bgra = 6,
    Rgba = 7,
    Bgrx = 8,
    /// Sound: 32 bit float samples, interleaved. See `audio.rs`.
    F32 = 100,
    /// Sound: 16 bit signed samples, interleaved.
    S16 = 101,
}

impl Format {
    pub const ALL: [Format; 8] = [
        Format::Nv12,
        Format::I420,
        Format::P010,
        Format::Yuy2,
        Format::Uyvy,
        Format::Bgra,
        Format::Rgba,
        Format::Bgrx,
    ];

    /// The GStreamer caps name, which is also the name used in errors.
    pub fn name(self) -> &'static str {
        match self {
            Format::Nv12 => "NV12",
            Format::I420 => "I420",
            Format::P010 => "P010_10LE",
            Format::Yuy2 => "YUY2",
            Format::Uyvy => "UYVY",
            Format::Bgra => "BGRA",
            Format::Rgba => "RGBA",
            Format::Bgrx => "BGRx",
            Format::F32 => "F32LE",
            Format::S16 => "S16LE",
        }
    }

    pub fn from_name(name: &str) -> Option<Format> {
        Format::ALL
            .into_iter()
            .chain(Format::AUDIO)
            .find(|f| f.name().eq_ignore_ascii_case(name))
    }

    pub fn from_code(code: u32) -> Option<Format> {
        Format::ALL.into_iter().chain(Format::AUDIO).find(|f| *f as u32 == code)
    }

    /// Per plane: bytes per pixel in a row, and the vertical subsampling shift.
    fn planes(self) -> &'static [(usize, usize, u32)] {
        // (bytes per sample group, horizontal divisor, vertical shift)
        match self {
            Format::Nv12 => &[(1, 1, 0), (2, 2, 1)],
            Format::I420 => &[(1, 1, 0), (1, 2, 1), (1, 2, 1)],
            Format::P010 => &[(2, 1, 0), (4, 2, 1)],
            Format::Yuy2 | Format::Uyvy => &[(2, 1, 0)],
            Format::Bgra | Format::Rgba | Format::Bgrx => &[(4, 1, 0)],
            // Sound has no planes; `Layout::new` refuses it before asking.
            Format::F32 | Format::S16 => &[],
        }
    }
}

/// Where each plane of a frame starts inside a slot, and its row stride.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub format: Format,
    pub width: u32,
    pub height: u32,
    pub n_planes: u32,
    pub offsets: [u64; 4],
    pub strides: [u32; 4],
    /// Bytes one frame takes, all planes and padding.
    pub size: u64,
    /// Frames per second as a fraction; 0/1 when unknown or variable.
    pub fps_n: u32,
    pub fps_d: u32,
}

impl Layout {
    /// The layout for a frame of this format and size.
    pub fn new(format: Format, width: u32, height: u32) -> Result<Layout, Error> {
        if format.is_audio() {
            return Err(Error::BadLayout(format!(
                "{} is sound, not a picture. Use Layout::audio",
                format.name()
            )));
        }
        if width == 0 || height == 0 || width > 16384 || height > 16384 {
            return Err(Error::BadLayout(format!(
                "a frame of {width}x{height} is outside 1x1 to 16384x16384. \
                 Give the size the decoder negotiated"
            )));
        }
        let mut layout = Layout {
            format,
            width,
            height,
            n_planes: 0,
            offsets: [0; 4],
            strides: [0; 4],
            size: 0,
            fps_n: 0,
            fps_d: 1,
        };
        let mut at = 0u64;
        for (i, &(bpp, hdiv, vshift)) in format.planes().iter().enumerate() {
            let cols = (width as usize).div_ceil(hdiv);
            let stride = (cols * bpp).next_multiple_of(ROW_ALIGN);
            let rows = (height as u64).div_ceil(1 << vshift);
            layout.offsets[i] = at;
            layout.strides[i] = stride as u32;
            at += rows * stride as u64;
            layout.n_planes += 1;
        }
        layout.size = at;
        Ok(layout)
    }

    /// The same layout at `n`/`d` frames a second.
    pub fn with_fps(mut self, n: u32, d: u32) -> Layout {
        (self.fps_n, self.fps_d) = (n, d.max(1));
        self
    }

    /// Rows in plane `i`.
    pub fn rows(&self, plane: usize) -> u32 {
        let (_, _, vshift) = self.format.planes()[plane];
        self.height.div_ceil(1 << vshift)
    }

    /// Bytes of real picture in one row of plane `i`, without padding.
    pub fn row_bytes(&self, plane: usize) -> usize {
        let (bpp, hdiv, _) = self.format.planes()[plane];
        (self.width as usize).div_ceil(hdiv) * bpp
    }
}

mod audio;
pub use audio::MAX_CHUNK_MS;

#[cfg(test)]
mod tests;
