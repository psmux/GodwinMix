//! A borrowed view of one decoded picture, in any of the formats a camera, a
//! frame bus slot or a decoded JPEG arrives in, so nothing is copied to look at
//! it.

mod sample;

use std::fmt;

/// How the bytes of a [`FrameRef`] are laid out.
///
/// The planar formats keep their planes one after another in the slice. For
/// `Nv12` the interleaved chroma plane has the same stride as the luma plane;
/// for `I420` each chroma plane has half the luma stride. That is how GStreamer,
/// the frame bus and y4m files lay them out when nothing pads between planes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PixelFormat {
    Rgb8,
    Bgra8,
    Nv12,
    I420,
    Gray8,
}

impl PixelFormat {
    /// The smallest stride, in bytes, a row of `width` pixels can have.
    pub fn min_stride(self, width: u32) -> usize {
        let w = width as usize;
        match self {
            PixelFormat::Rgb8 => w * 3,
            PixelFormat::Bgra8 => w * 4,
            PixelFormat::Nv12 | PixelFormat::I420 | PixelFormat::Gray8 => w,
        }
    }

    /// The number of bytes a whole frame needs for this stride and height.
    pub fn frame_len(self, stride: usize, height: u32) -> usize {
        let h = height as usize;
        let ch = h.div_ceil(2);
        match self {
            PixelFormat::Rgb8 | PixelFormat::Bgra8 | PixelFormat::Gray8 => stride * h,
            PixelFormat::Nv12 => stride * h + stride * ch,
            PixelFormat::I420 => stride * h + 2 * stride.div_ceil(2) * ch,
        }
    }
}

/// One picture: its size, the bytes per luma (or packed) row, the format and
/// the pixels themselves, borrowed.
#[derive(Clone, Copy)]
pub struct FrameRef<'a> {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: PixelFormat,
    pub data: &'a [u8],
}

impl fmt::Debug for FrameRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "FrameRef({}x{} {:?} stride {})", self.width, self.height, self.format, self.stride)
    }
}

/// Why a [`FrameRef`] could not be made.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameError {
    pub width: u32,
    pub height: u32,
    pub stride: usize,
    pub format: PixelFormat,
    pub needed: usize,
    pub got: usize,
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.width == 0 || self.height == 0 {
            return write!(f, "a frame of {}x{} has no pixels; pass the decoded size", self.width, self.height);
        }
        write!(
            f,
            "a {}x{} {:?} frame with stride {} needs {} bytes but the slice holds {}; \
             pass the whole buffer with every plane, and the stride the decoder reported \
             (at least {})",
            self.width,
            self.height,
            self.format,
            self.stride,
            self.needed,
            self.got,
            self.format.min_stride(self.width)
        )
    }
}

impl std::error::Error for FrameError {}

impl<'a> FrameRef<'a> {
    /// A checked view: the slice must hold the whole frame for this stride.
    pub fn new(
        width: u32,
        height: u32,
        stride: usize,
        format: PixelFormat,
        data: &'a [u8],
    ) -> Result<Self, FrameError> {
        let frame = Self { width, height, stride, format, data };
        frame.validate().map(|_| frame)
    }

    /// A view with the tightest stride for the format.
    pub fn packed(width: u32, height: u32, format: PixelFormat, data: &'a [u8]) -> Result<Self, FrameError> {
        Self::new(width, height, format.min_stride(width), format, data)
    }

    /// Checks the size, the stride and the slice length agree. Every reader in
    /// this crate checks this first and treats a frame that fails it as empty.
    pub fn validate(&self) -> Result<(), FrameError> {
        let needed = self.format.frame_len(self.stride, self.height);
        let ok = self.width > 0
            && self.height > 0
            && self.stride >= self.format.min_stride(self.width)
            && self.data.len() >= needed;
        if ok {
            return Ok(());
        }
        Err(FrameError {
            width: self.width,
            height: self.height,
            stride: self.stride,
            format: self.format,
            needed,
            got: self.data.len(),
        })
    }

    /// Width over height, in pixels.
    pub fn aspect(&self) -> f32 {
        self.width as f32 / self.height.max(1) as f32
    }
}
