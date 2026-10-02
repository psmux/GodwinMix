use super::*;

/// A 16x8 I420 frame filled with one colour.
fn frame(y: u8, u: u8, v: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (vec![y; 16 * 8], vec![u; 8 * 4], vec![v; 8 * 4])
}

/// A picture `w` wide and `h` high, the left half opaque in one colour and
/// the right half fully transparent.
fn half(w: usize, h: usize, (y, u, v): (u8, u8, u8)) -> Vec<u8> {
    let mut data = vec![0u8; w * h * 4];
    for row in 0..h {
        for col in 0..w {
            let p = (row * w + col) * 4;
            data[p..p + 4].copy_from_slice(&[if col < w / 2 { 255 } else { 0 }, y, u, v]);
        }
    }
    data
}

fn planes<'a>(f: &'a mut (Vec<u8>, Vec<u8>, Vec<u8>)) -> Planes<'a> {
    Planes { y: &mut f.0, u: &mut f.1, v: &mut f.2, strides: [16, 8, 8], width: 16, height: 8 }
}

#[test]
fn an_opaque_pixel_replaces_and_a_transparent_one_leaves_the_frame_alone() {
    let mut f = frame(41, 240, 110);
    let pic = half(8, 8, (81, 90, 240));
    let d = Draw { window: Rect::new(0, 0, 8, 8), to: Rect::new(0, 0, 8, 8), clip: Rect::new(0, 0, 16, 8), alpha: 255 };
    draw(&mut planes(&mut f), &Source { data: &pic, stride: 32 }, &d);
    assert_eq!(f.0[0], 81, "opaque half takes the picture's luma");
    assert_eq!(f.0[3], 81);
    assert_eq!(f.0[4], 41, "transparent half keeps what was under it");
    assert_eq!(f.0[12], 41, "outside the picture is untouched");
    assert_eq!((f.1[0], f.2[0]), (90, 240), "opaque chroma replaced");
    assert_eq!((f.1[3], f.2[3]), (240, 110), "transparent chroma untouched");
}

#[test]
fn half_alpha_mixes_halfway() {
    let mut f = frame(0, 128, 128);
    let pic = half(4, 2, (200, 128, 128));
    let d = Draw { window: Rect::new(0, 0, 4, 2), to: Rect::new(0, 0, 4, 2), clip: Rect::new(0, 0, 16, 8), alpha: 128 };
    draw(&mut planes(&mut f), &Source { data: &pic, stride: 16 }, &d);
    assert!((99..=101).contains(&f.0[0]), "got {}", f.0[0]);
}

#[test]
fn a_window_crops_and_a_clip_stops_drawing_at_the_box() {
    let mut f = frame(10, 128, 128);
    // A strip 8 wide whose left half is opaque: a window starting at 2 sees
    // two opaque columns then transparent ones.
    let pic = half(8, 2, (99, 128, 128));
    let d = Draw { window: Rect::new(2, 0, 4, 2), to: Rect::new(4, 0, 4, 2), clip: Rect::new(0, 0, 5, 8), alpha: 255 };
    draw(&mut planes(&mut f), &Source { data: &pic, stride: 32 }, &d);
    assert_eq!(&f.0[3..7], &[10, 99, 10, 10], "only column 4 is inside both the window and the clip");
}

#[test]
fn a_picture_off_the_frame_draws_nothing_and_does_not_panic() {
    let mut f = frame(10, 128, 128);
    let pic = half(4, 4, (99, 128, 128));
    for to in [Rect::new(-10, 0, 4, 4), Rect::new(20, 0, 4, 4), Rect::new(0, 9, 4, 4), Rect::new(-1, -1, 4, 4)] {
        let d = Draw { window: Rect::new(0, 0, 4, 4), to, clip: Rect::new(-100, -100, 400, 400), alpha: 255 };
        draw(&mut planes(&mut f), &Source { data: &pic, stride: 16 }, &d);
    }
    assert_eq!(f.0[0], 99, "the one partly on the frame drew its opaque corner");
    assert_eq!(f.0[16 * 7 + 15], 10);
}

#[test]
fn a_stretched_picture_fills_its_box() {
    let mut f = frame(10, 128, 128);
    let pic: Vec<u8> = [255u8, 200, 128, 128].repeat(4);
    let d = Draw { window: Rect::new(0, 0, 2, 2), to: Rect::new(0, 0, 16, 8), clip: Rect::new(0, 0, 16, 8), alpha: 255 };
    draw(&mut planes(&mut f), &Source { data: &pic, stride: 8 }, &d);
    assert!(f.0.iter().all(|&y| y == 200));
}
