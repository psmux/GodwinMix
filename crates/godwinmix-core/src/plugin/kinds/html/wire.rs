//! The frames the browser renderer writes in graphic mode, as bytes: the
//! header, a box placed on its page, and a patch copied into a box. See
//! `browser/src/graphic.rs` for the writing side.

use crate::overlay::picture::{Area, Picture};

pub const MAGIC: &[u8; 4] = b"GMXF";
pub const PATCH: &[u8; 4] = b"GMXP";
pub const WHOLE: &[u8; 4] = b"GMXI";
pub const HEADER: usize = 28;
/// The largest page the renderer is ever asked for, as a check on a header.
pub const MAX_SIDE: u32 = 8192;
/// The page size and the box a header describes.
pub fn parse(h: &[u8; HEADER]) -> Option<((u32, u32), Area)> {
    if &h[..4] != MAGIC && &h[..4] != PATCH && &h[..4] != WHOLE {
        return None;
    }
    let n = |i: usize| u32::from_le_bytes([h[4 + i * 4], h[5 + i * 4], h[6 + i * 4], h[7 + i * 4]]);
    let (pw, ph, x, y, w, h) = (n(0), n(1), n(2), n(3), n(4), n(5));
    let fits = pw <= MAX_SIDE && ph <= MAX_SIDE && x.saturating_add(w) <= pw && y.saturating_add(h) <= ph;
    fits.then_some(((pw, ph), Area { x, y, w, h }))
}

/// Copy a patch covering `part` of the page into `pixels`, which hold the
/// box `at`. False when the patch is not inside the box.
pub fn apply(pixels: &mut [u8], at: Area, patch: &[u8], part: Area) -> bool {
    let fits = part.x >= at.x && part.y >= at.y && part.x + part.w <= at.x + at.w && part.y + part.h <= at.y + at.h;
    if !fits {
        return false;
    }
    let (row, stride) = (part.w as usize * 4, at.w as usize * 4);
    for y in 0..part.h as usize {
        let to = (part.y - at.y) as usize * stride + y * stride + (part.x - at.x) as usize * 4;
        pixels[to..to + row].copy_from_slice(&patch[y * row..(y + 1) * row]);
    }
    true
}

/// A box of AYUV as a picture placed on its page. None for an empty page.
pub fn picture(data: Vec<u8>, page: (u32, u32), area: Area) -> Option<Picture> {
    if area.w == 0 || area.h == 0 {
        return None;
    }
    Some(Picture { within: Some(area), ..Picture::from_ayuv(data, area.w, area.h, page) })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_header_reads_back_and_a_bad_one_is_refused() {
        let mut h = [0u8; HEADER];
        h[..4].copy_from_slice(MAGIC);
        for (i, v) in [1920u32, 1080, 100, 800, 900, 200].iter().enumerate() {
            h[4 + i * 4..8 + i * 4].copy_from_slice(&v.to_le_bytes());
        }
        assert_eq!(parse(&h), Some(((1920, 1080), Area { x: 100, y: 800, w: 900, h: 200 })));
        h[20..24].copy_from_slice(&2000u32.to_le_bytes());
        assert_eq!(parse(&h), None, "a box off the page");
        h[0] = b'X';
        assert_eq!(parse(&h), None);
    }

    #[test]
    fn a_patch_lands_inside_the_box_it_belongs_to() {
        let at = Area { x: 10, y: 10, w: 4, h: 4 };
        let mut pixels = vec![0u8; 64];
        assert!(apply(&mut pixels, at, &[9u8; 8], Area { x: 11, y: 12, w: 1, h: 2 }));
        assert_eq!(&pixels[2 * 16 + 4..2 * 16 + 8], &[9, 9, 9, 9]);
        assert_eq!(&pixels[3 * 16 + 4..3 * 16 + 8], &[9, 9, 9, 9]);
        assert_eq!(pixels.iter().filter(|b| **b == 9).count(), 8);
        assert!(!apply(&mut pixels, at, &[9u8; 4], Area { x: 20, y: 12, w: 1, h: 1 }));
    }

    #[test]
    fn an_empty_box_is_no_picture_and_a_box_is_placed_on_its_page() {
        assert!(picture(Vec::new(), (1920, 1080), Area { x: 0, y: 0, w: 0, h: 0 }).is_none());
        let p = picture(vec![255; 16], (1920, 1080), Area { x: 10, y: 20, w: 2, h: 2 }).unwrap();
        assert_eq!((p.natural, p.within, p.width), ((1920, 1080), Some(Area { x: 10, y: 20, w: 2, h: 2 }), 2));
    }
}
