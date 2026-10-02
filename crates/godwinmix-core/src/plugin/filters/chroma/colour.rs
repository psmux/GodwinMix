//! Colour arithmetic in the canvas's own terms: BT.709, limited range.
//!
//! The key works on the frame as it arrives, in I420, so the key colour a
//! person picks in RGB is turned into the same Y, U and V the frame holds.

/// RGB to Y, U, V, BT.709 limited range.
pub fn rgb_to_yuv(rgb: [u8; 3]) -> (u8, u8, u8) {
    let [r, g, b] = rgb.map(|c| c as f32 / 255.0);
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let u = (b - y) / 1.8556;
    let v = (r - y) / 1.5748;
    let clamp = |x: f32| x.round().clamp(0.0, 255.0) as u8;
    (clamp(16.0 + 219.0 * y), clamp(128.0 + 224.0 * u), clamp(128.0 + 224.0 * v))
}

/// The inverse, for reporting a colour found in the frame.
pub fn yuv_to_rgb(y: u8, u: u8, v: u8) -> [u8; 3] {
    let y = (y as f32 - 16.0) / 219.0;
    let u = (u as f32 - 128.0) / 224.0;
    let v = (v as f32 - 128.0) / 224.0;
    let r = y + 1.5748 * v;
    let b = y + 1.8556 * u;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    [r, g, b].map(|c| (c * 255.0).round().clamp(0.0, 255.0) as u8)
}

/// `#rrggbb`, or the same without the hash.
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    if hex.len() != 6 || !hex.is_ascii() {
        return None;
    }
    let at = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    Some([at(0)?, at(2)?, at(4)?])
}

pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn green_and_blue_land_where_bt709_puts_them() {
        assert_eq!(rgb_to_yuv([0, 255, 0]), (173, 42, 26));
        assert_eq!(rgb_to_yuv([0, 0, 255]), (32, 240, 118));
        assert_eq!(rgb_to_yuv([255, 255, 255]), (235, 128, 128));
    }

    #[test]
    fn a_colour_survives_the_round_trip_within_a_step_or_two() {
        for rgb in [[48, 176, 80], [20, 60, 200], [230, 180, 150]] {
            let (y, u, v) = rgb_to_yuv(rgb);
            let back = yuv_to_rgb(y, u, v);
            for i in 0..3 {
                assert!((back[i] as i32 - rgb[i] as i32).abs() <= 2, "{rgb:?} came back as {back:?}");
            }
        }
    }

    #[test]
    fn hex_reads_with_or_without_the_hash() {
        assert_eq!(parse_hex("#30b050"), Some([0x30, 0xb0, 0x50]));
        assert_eq!(parse_hex("30B050"), Some([0x30, 0xb0, 0x50]));
        assert_eq!(parse_hex("green"), None);
        assert_eq!(to_hex([0x30, 0xb0, 0x50]), "#30b050");
    }
}
