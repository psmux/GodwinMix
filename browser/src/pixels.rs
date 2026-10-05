//! The pixel work of graphic mode: finding the painted box, and turning
//! Chromium's premultiplied BGRA into the AYUV the overlay board draws, or
//! the I420 the compositor takes for a design that covers the whole picture.

use crate::graphic::Area;

/// Copy `area` of a page `width` wide out into `into`, `area.w * 4` a row.
pub fn copy_area(page: &[u8], width: i32, area: Area, into: &mut Vec<u8>) {
    into.clear();
    let stride = width as usize * 4;
    for y in area.y..area.y + area.h {
        let at = y as usize * stride + area.x as usize * 4;
        into.extend_from_slice(&page[at..at + area.w as usize * 4]);
    }
}

/// The box around every pixel of `region` with any alpha, in page pixels.
pub fn content(region: &[u8], area: Area) -> Area {
    let row = area.w.max(0) as usize * 4;
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, -1, -1);
    for (y, line) in region.chunks_exact(row.max(4)).enumerate().take(area.h.max(0) as usize) {
        let Some(first) = line.chunks_exact(4).position(|px| px[3] != 0) else { continue };
        let last = line.chunks_exact(4).rposition(|px| px[3] != 0).unwrap_or(first);
        x0 = x0.min(first as i32);
        x1 = x1.max(last as i32 + 1);
        y0 = y0.min(y as i32);
        y1 = y as i32 + 1;
    }
    if x1 < 0 {
        return Area::default();
    }
    Area { x: area.x + x0, y: area.y + y0, w: x1 - x0, h: y1 - y0 }
}

/// Whether `a` lies inside `b` without touching its edges, so the box
/// around the page's content is the same after `a` changed.
pub fn inside(a: Area, b: Area) -> bool {
    !a.is_empty() && !b.is_empty() && a.x > b.x && a.y > b.y && a.x + a.w < b.x + b.w && a.y + a.h < b.y + b.h
}

/// One frame on the wire: the header and `tight` of `region` (which covers
/// `area`) in AYUV.
pub fn encode(region: &[u8], area: Area, tight: Area, page: (i32, i32)) -> Vec<u8> {
    encode_as(b"GMXF", region, area, tight, page)
}

pub fn encode_as(magic: &[u8; 4], region: &[u8], area: Area, tight: Area, page: (i32, i32)) -> Vec<u8> {
    let mut out = Vec::with_capacity(28 + (tight.w.max(0) * tight.h.max(0) * 4) as usize);
    out.extend_from_slice(magic);
    for v in [page.0, page.1, tight.x, tight.y, tight.w.max(0), tight.h.max(0)] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    let stride = area.w as usize * 4;
    for y in tight.y..tight.y + tight.h {
        let at = (y - area.y) as usize * stride + (tight.x - area.x) as usize * 4;
        for px in region[at..at + tight.w as usize * 4].chunks_exact(4) {
            out.extend_from_slice(&ayuv(px));
        }
    }
    out
}

/// One premultiplied BGRA pixel as straight alpha AYUV, BT.709 limited range.
pub fn ayuv(px: &[u8]) -> [u8; 4] {
    let a = px[3] as i32;
    if a == 0 {
        return [0, 16, 128, 128];
    }
    let un = |c: u8| if a == 255 { c as i32 } else { (c as i32 * 255 / a).min(255) };
    let (b, g, r) = (un(px[0]), un(px[1]), un(px[2]));
    // BT.709 in 8 bit fixed point, scaled by 256.
    let y = 16 + ((47 * r + 157 * g + 16 * b + 128) >> 8);
    let u = 128 + ((-26 * r - 86 * g + 112 * b + 128) >> 8);
    let v = 128 + ((112 * r - 102 * g - 10 * b + 128) >> 8);
    [a as u8, y.clamp(16, 235) as u8, u.clamp(16, 240) as u8, v.clamp(16, 240) as u8]
}

/// A whole page as I420, BT.709 limited range, behind a `GMXI` header: for
/// a design that covers the picture, which goes to the compositor like a
/// camera and needs no alpha. Anything not opaque is taken as over black.
pub fn encode_i420(page: &[u8], w: i32, h: i32) -> Vec<u8> {
    let (w, h) = (w.max(0) as usize & !1, h.max(0) as usize & !1);
    let stride = w * 4;
    let mut out = Vec::with_capacity(28 + w * h * 3 / 2);
    out.extend_from_slice(b"GMXI");
    for v in [w, h, 0, 0, w, h] {
        out.extend_from_slice(&(v as u32).to_le_bytes());
    }
    let rgb = |x: usize, y: usize| {
        let px = &page[y * stride + x * 4..y * stride + x * 4 + 4];
        (px[2] as i32, px[1] as i32, px[0] as i32)
    };
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = rgb(x, y);
            out.push((16 + ((47 * r + 157 * g + 16 * b + 128) >> 8)) as u8);
        }
    }
    let mut v_plane = Vec::with_capacity(w * h / 4);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            let (mut r, mut g, mut b) = (0, 0, 0);
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = rgb(x + dx, y + dy);
                (r, g, b) = (r + p.0, g + p.1, b + p.2);
            }
            let (r, g, b) = (r / 4, g / 4, b / 4);
            out.push((128 + ((-26 * r - 86 * g + 112 * b + 128) >> 8)).clamp(16, 240) as u8);
            v_plane.push((128 + ((112 * r - 102 * g - 10 * b + 128) >> 8)).clamp(16, 240) as u8);
        }
    }
    out.extend_from_slice(&v_plane);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graphic::Area;

    #[test]
    fn white_black_and_clear_convert_to_the_canvas_levels() {
        assert_eq!(ayuv(&[255, 255, 255, 255]), [255, 235, 128, 128]);
        assert_eq!(ayuv(&[0, 0, 0, 255]), [255, 16, 128, 128]);
        assert_eq!(ayuv(&[0, 0, 0, 0]), [0, 16, 128, 128]);
        // Half covered white, premultiplied, is still white at half alpha.
        assert_eq!(ayuv(&[128, 128, 128, 128])[1], 235);
    }

    #[test]
    fn a_whole_white_page_is_white_in_i420() {
        let page = vec![255u8; 4 * 4 * 2 * 4];
        let f = encode_i420(&page, 4, 4);
        assert_eq!(&f[..4], b"GMXI");
        assert_eq!(f.len(), 28 + 16 + 4 + 4);
        assert!(f[28..44].iter().all(|y| *y == 235) && f[44..].iter().all(|c| *c == 128), "{:?}", &f[28..]);
    }

    #[test]
    fn only_the_painted_box_is_sent() {
        let area = Area { x: 10, y: 20, w: 4, h: 3 };
        let mut region = vec![0u8; 4 * 3 * 4];
        region[(4 + 2) * 4 + 3] = 255; // row 1, column 2
        let tight = content(&region, area);
        assert_eq!(tight, Area { x: 12, y: 21, w: 1, h: 1 });
        let frame = encode(&region, area, tight, (100, 50));
        assert_eq!(&frame[..4], b"GMXF");
        assert_eq!(frame.len(), 28 + 4);
        assert_eq!(content(&vec![0u8; 48], area), Area::default());
    }
}
