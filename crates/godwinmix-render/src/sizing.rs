//! Filling in a size or a bitrate a request left out. Deterministic, so two
//! requests that leave out the same things land on one encoder.

use godwinmix_protocol::rendition::{VideoCodec, VideoShape};

/// Fills a missing side from the source's aspect ratio, rounded to even.
pub fn size(width: Option<u32>, height: Option<u32>, src: &VideoShape) -> (u32, u32) {
    let scale = |n: u32, num: u32, den: u32| even(u64::from(n) * u64::from(num) / u64::from(den.max(1)));
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, scale(w, src.height, src.width)),
        (None, Some(h)) => (scale(h, src.width, src.height), h),
        (None, None) => (src.width, src.height),
    }
}

fn even(n: u64) -> u32 {
    let n = u32::try_from(n).unwrap_or(u32::MAX - 1);
    (n + (n & 1)).max(2)
}

/// A bitrate for an output that named none, from bits per pixel per codec,
/// rounded to 100 kbit/s so equal shapes always share an encoder. H.264
/// 1080p30 comes to 6200 kbit/s and 720p30 to 2800.
pub fn default_kbps(shape: &VideoShape) -> u32 {
    let bpp = match shape.codec {
        VideoCodec::H264 => 0.1,
        VideoCodec::Mpeg2 => 0.2,
        VideoCodec::Vp8 => 0.11,
        _ => 0.06,
    };
    let px = f64::from(shape.width) * f64::from(shape.height) * shape.fps.as_f64();
    let kbps = (px * bpp / 1000.0 / 100.0).round() * 100.0;
    (kbps as u32).max(100)
}
