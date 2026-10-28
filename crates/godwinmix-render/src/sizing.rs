//! Filling in a size or a bitrate a request left out. Deterministic, so two
//! requests that leave out the same things land on one encoder.

use godwinmix_protocol::rendition::{VideoCodec, VideoShape, VideoWant};

/// Fills a missing side from the source's aspect ratio, rounded to even.
pub fn size(width: Option<u32>, height: Option<u32>, src: &VideoShape) -> (u32, u32) {
    let scale =
        |n: u32, num: u32, den: u32| even(u64::from(n) * u64::from(num) / u64::from(den.max(1)));
    match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (w, scale(w, src.height, src.width)),
        (None, Some(h)) => (scale(h, src.width, src.height), h),
        (None, None) => (src.width, src.height),
    }
}

/// The size and bitrate after `match_source`: a source taller than the
/// asked height is made at its own height, up to `max_height`, the width
/// grown to keep the frame's shape, and the bitrate the one given for
/// `max_height`, else the asked one grown by the area. Without it, or for a
/// source no taller, the size and bitrate asked for.
pub fn matched(want: &VideoWant, width: u32, height: u32, source_height: u32) -> (u32, u32, Option<u32>) {
    let asked = want.bitrate_kbps;
    let Some(m) = want.match_source.filter(|m| source_height > height && m.max_height > height && height > 0) else {
        return (width, height, asked);
    };
    let to = source_height.min(m.max_height) & !1;
    let wide = even(u64::from(width) * u64::from(to) / u64::from(height));
    let area = |w: u32, h: u32| f64::from(w) * f64::from(h);
    let kbps = match (m.bitrate_kbps, asked) {
        (Some(top), _) if to == m.max_height => Some(top),
        // Between the two sizes: the top bitrate scaled down by area.
        (Some(top), _) => {
            let full = area(even(u64::from(width) * u64::from(m.max_height) / u64::from(height)), m.max_height);
            Some((f64::from(top) * area(wide, to) / full).round() as u32)
        }
        (None, Some(k)) => Some((f64::from(k) * area(wide, to) / area(width, height)).round() as u32),
        (None, None) => None,
    };
    (wide, to, kbps)
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

#[cfg(test)]
mod tests {
    use super::*;
    use godwinmix_protocol::rendition::MatchSource;

    fn want(max_height: u32, top: Option<u32>) -> VideoWant {
        VideoWant { bitrate_kbps: Some(1500), match_source: Some(MatchSource { max_height, bitrate_kbps: top }), ..VideoWant::default() }
    }

    #[test]
    fn a_taller_source_is_made_at_its_own_size_up_to_the_limit() {
        assert_eq!(matched(&want(1080, Some(4500)), 1280, 720, 1080), (1920, 1080, Some(4500)));
        // No taller than the rendition: as asked.
        assert_eq!(matched(&want(1080, Some(4500)), 1280, 720, 720), (1280, 720, Some(1500)));
        assert_eq!(matched(&want(1080, Some(4500)), 1280, 720, 480), (1280, 720, Some(1500)));
        // Taller than the limit: the limit.
        assert_eq!(matched(&want(1080, Some(4500)), 1280, 720, 2160), (1920, 1080, Some(4500)));
        // No bitrate for the limit: the asked one grown by the area.
        assert_eq!(matched(&want(1080, None), 1280, 720, 1080), (1920, 1080, Some(3375)));
        // No match_source at all.
        let plain = VideoWant { bitrate_kbps: Some(1500), ..VideoWant::default() };
        assert_eq!(matched(&plain, 1280, 720, 1080), (1280, 720, Some(1500)));
    }
}
