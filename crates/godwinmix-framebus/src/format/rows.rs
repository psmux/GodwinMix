//! Rows and rates of a picture layout.

use super::Layout;

impl Layout {
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
