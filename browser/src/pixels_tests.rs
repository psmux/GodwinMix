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
