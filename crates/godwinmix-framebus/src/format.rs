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
        }
    }

    pub fn from_name(name: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.name().eq_ignore_ascii_case(name))
    }

    pub fn from_code(code: u32) -> Option<Format> {
        Format::ALL.into_iter().find(|f| *f as u32 == code)
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
}

impl Layout {
    /// The layout for a frame of this format and size.
    pub fn new(format: Format, width: u32, height: u32) -> Result<Layout, Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nv12_1080p_is_two_planes_with_aligned_rows() {
        let l = Layout::new(Format::Nv12, 1920, 1080).unwrap();
        assert_eq!(l.n_planes, 2);
        assert_eq!(l.strides[..2], [1920, 1920]);
        assert_eq!(l.offsets[1], 1920 * 1080);
        assert_eq!(l.size, 1920 * 1080 * 3 / 2);
        assert_eq!(l.row_bytes(1), 1920);
        assert_eq!(l.rows(1), 540);
    }

    #[test]
    fn i420_odd_size_rounds_chroma_up() {
        let l = Layout::new(Format::I420, 641, 361).unwrap();
        assert_eq!(l.n_planes, 3);
        assert_eq!(l.row_bytes(1), 321);
        assert_eq!(l.rows(2), 181);
        assert_eq!(l.strides[0] as usize % ROW_ALIGN, 0);
        assert_eq!(l.size, l.offsets[2] + 181 * l.strides[2] as u64);
    }

    #[test]
    fn names_round_trip_and_zero_size_is_refused() {
        for f in Format::ALL {
            assert_eq!(Format::from_name(f.name()), Some(f));
            assert_eq!(Format::from_code(f as u32), Some(f));
        }
        assert!(Layout::new(Format::Nv12, 0, 10).is_err());
    }
}
