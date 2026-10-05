use super::*;

/// A 16x8 I420 frame of one colour.
fn frame(y: u8, u: u8, v: u8) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    (vec![y; 16 * 8], vec![u; 8 * 4], vec![v; 8 * 4])
}

/// An opaque 16x8 AYUV picture, the left half `left` and the right half black.
fn picture(left: (u8, u8, u8)) -> Vec<u8> {
    let mut data = vec![0u8; 16 * 8 * 4];
    for row in 0..8 {
        for col in 0..16 {
            let (y, u, v) = if col < 8 { left } else { (16, 128, 128) };
            data[(row * 16 + col) * 4..][..4].copy_from_slice(&[255, y, u, v]);
        }
    }
    data
}

fn run(f: &mut (Vec<u8>, Vec<u8>, Vec<u8>), pic: &[u8], mode: Mode, alpha: u8) {
    let mut p = Planes { y: &mut f.0, u: &mut f.1, v: &mut f.2, strides: [16, 8, 8], width: 16, height: 8 };
    let whole = Rect::new(0, 0, 16, 8);
    draw(&mut p, &Source { data: pic, stride: 64 }, &Draw { window: whole, to: whole, clip: whole, alpha }, mode);
}

#[test]
fn black_leaves_the_picture_alone_under_screen_add_and_luma() {
    for mode in [Mode::Screen, Mode::Add, Mode::Luma] {
        let mut f = frame(120, 90, 160);
        run(&mut f, &picture((200, 100, 170)), mode, 255);
        assert_eq!((f.0[12], f.1[6], f.2[6]), (120, 90, 160), "{mode:?} changed a pixel under black");
    }
}

#[test]
fn screen_lightens_and_never_goes_past_white() {
    let mut f = frame(126, 128, 128);
    run(&mut f, &picture((126, 128, 128)), Mode::Screen, 255);
    // Half grey screened on half grey is three quarters.
    let three_quarters = 16 + 219 * 3 / 4;
    assert!((f.0[0] as i32 - three_quarters).abs() <= 2, "screen of two greys was {}", f.0[0]);
    let mut white = frame(235, 128, 128);
    run(&mut white, &picture((235, 128, 128)), Mode::Screen, 255);
    assert_eq!(white.0[0], 235, "screen cannot go past white");
}

#[test]
fn add_adds_the_light_and_a_warm_tint() {
    let mut f = frame(60, 128, 128);
    run(&mut f, &picture((60, 100, 170)), Mode::Add, 255);
    assert!(f.0[0] > 100, "add should brighten, was {}", f.0[0]);
    assert!(f.1[0] < 128 && f.2[0] > 128, "add carries the clip's colour: {} {}", f.1[0], f.2[0]);
}

#[test]
fn luma_key_draws_white_solid_and_opacity_scales_every_mode() {
    let mut f = frame(40, 128, 128);
    run(&mut f, &picture((235, 128, 128)), Mode::Luma, 255);
    assert_eq!(f.0[0], 235, "white is solid under a luma key");
    let mut half = frame(40, 128, 128);
    run(&mut half, &picture((235, 128, 128)), Mode::Luma, 128);
    assert!(half.0[0] > 120 && half.0[0] < 150, "half opacity lands half way: {}", half.0[0]);
}
