//! Reading single pixels and whole downscaled pictures out of any format.
//! Every function here expects a validated frame and never allocates.

use super::{FrameRef, PixelFormat};

impl FrameRef<'_> {
    fn chroma(&self, x: usize, y: usize) -> (i32, i32) {
        let luma = self.stride * self.height as usize;
        let (cx, cy) = (x / 2, y / 2);
        match self.format {
            PixelFormat::Nv12 => {
                let i = luma + cy * self.stride + cx * 2;
                (self.data[i] as i32, self.data[i + 1] as i32)
            }
            PixelFormat::I420 => {
                let cs = self.stride.div_ceil(2);
                let plane = cs * (self.height as usize).div_ceil(2);
                let i = luma + cy * cs + cx;
                (self.data[i] as i32, self.data[i + plane] as i32)
            }
            _ => (128, 128),
        }
    }

    /// The brightness, 0 to 255, at pixel `(x, y)`. Out of range coordinates
    /// are clamped to the edge.
    #[inline]
    pub fn luma_at(&self, x: u32, y: u32) -> u8 {
        let x = x.min(self.width - 1) as usize;
        let y = y.min(self.height - 1) as usize;
        let row = y * self.stride;
        match self.format {
            PixelFormat::Gray8 | PixelFormat::Nv12 | PixelFormat::I420 => self.data[row + x],
            PixelFormat::Rgb8 => {
                let p = &self.data[row + x * 3..row + x * 3 + 3];
                luma(p[0], p[1], p[2])
            }
            PixelFormat::Bgra8 => {
                let p = &self.data[row + x * 4..row + x * 4 + 3];
                luma(p[2], p[1], p[0])
            }
        }
    }

    /// The colour at pixel `(x, y)` as red, green and blue, 0 to 255. YUV is
    /// converted with the BT.601 limited range matrix.
    #[inline]
    pub fn rgb_at(&self, x: u32, y: u32) -> [u8; 3] {
        let xu = x.min(self.width - 1) as usize;
        let yu = y.min(self.height - 1) as usize;
        let row = yu * self.stride;
        match self.format {
            PixelFormat::Rgb8 => {
                let p = &self.data[row + xu * 3..row + xu * 3 + 3];
                [p[0], p[1], p[2]]
            }
            PixelFormat::Bgra8 => {
                let p = &self.data[row + xu * 4..row + xu * 4 + 3];
                [p[2], p[1], p[0]]
            }
            PixelFormat::Gray8 => {
                let v = self.data[row + xu];
                [v, v, v]
            }
            PixelFormat::Nv12 | PixelFormat::I420 => {
                let (u, v) = self.chroma(xu, yu);
                yuv_to_rgb(self.data[row + xu] as i32, u, v)
            }
        }
    }

    /// Scales the brightness of the whole frame into `dst`, a `dw` by `dh`
    /// grey picture, by nearest neighbour. `dst` must hold `dw * dh` bytes.
    pub fn downscale_luma(&self, dst: &mut [u8], dw: usize, dh: usize) {
        assert!(dst.len() >= dw * dh, "dst holds {} bytes, needs {}", dst.len(), dw * dh);
        let (sw, sh) = (self.width as usize, self.height as usize);
        for y in 0..dh {
            let sy = ((2 * y + 1) * sh / (2 * dh)) as u32;
            let out = &mut dst[y * dw..(y + 1) * dw];
            for (x, px) in out.iter_mut().enumerate() {
                let sx = ((2 * x + 1) * sw / (2 * dw)) as u32;
                *px = self.luma_at(sx, sy);
            }
        }
    }

    /// Scales the whole frame into `dst` as packed RGB, `dw` by `dh`, by
    /// nearest neighbour. `dst` must hold `dw * dh * 3` bytes.
    pub fn downscale_rgb(&self, dst: &mut [u8], dw: usize, dh: usize) {
        assert!(dst.len() >= dw * dh * 3, "dst holds {} bytes, needs {}", dst.len(), dw * dh * 3);
        let (sw, sh) = (self.width as usize, self.height as usize);
        for y in 0..dh {
            let sy = ((2 * y + 1) * sh / (2 * dh)) as u32;
            for x in 0..dw {
                let sx = ((2 * x + 1) * sw / (2 * dw)) as u32;
                let i = (y * dw + x) * 3;
                dst[i..i + 3].copy_from_slice(&self.rgb_at(sx, sy));
            }
        }
    }
}

#[inline]
fn luma(r: u8, g: u8, b: u8) -> u8 {
    (((66 * r as u32 + 129 * g as u32 + 25 * b as u32 + 128) >> 8) + 16) as u8
}

#[inline]
fn yuv_to_rgb(y: i32, u: i32, v: i32) -> [u8; 3] {
    let c = 298 * (y - 16);
    let (d, e) = (u - 128, v - 128);
    let r = (c + 409 * e + 128) >> 8;
    let g = (c - 100 * d - 208 * e + 128) >> 8;
    let b = (c + 516 * d + 128) >> 8;
    [r.clamp(0, 255) as u8, g.clamp(0, 255) as u8, b.clamp(0, 255) as u8]
}

#[cfg(test)]
#[path = "sample_tests.rs"]
mod tests;
