use super::*;

#[test]
fn every_format_reads_the_same_colour() {
    let rgb = [200u8, 40, 40].repeat(4);
    let f = FrameRef::packed(2, 2, PixelFormat::Rgb8, &rgb).unwrap();
    assert_eq!(f.rgb_at(1, 1), [200, 40, 40]);
    let bgra = [40u8, 40, 200, 255].repeat(4);
    let f = FrameRef::packed(2, 2, PixelFormat::Bgra8, &bgra).unwrap();
    assert_eq!(f.rgb_at(0, 1), [200, 40, 40]);
    assert_eq!(f.luma_at(0, 0), FrameRef::packed(2, 2, PixelFormat::Rgb8, &rgb).unwrap().luma_at(0, 0));
    // Y, U and V for about (200, 40, 40) in BT.601 limited range.
    let (y, u, v) = (92u8, 104u8, 198u8);
    let nv12 = [y, y, y, y, u, v];
    let f = FrameRef::packed(2, 2, PixelFormat::Nv12, &nv12).unwrap();
    let i420 = [y, y, y, y, u, v];
    let g = FrameRef::packed(2, 2, PixelFormat::I420, &i420).unwrap();
    for c in [f.rgb_at(1, 0), g.rgb_at(1, 0)] {
        assert!((c[0] as i32 - 200).abs() < 8 && (c[1] as i32 - 40).abs() < 8, "{c:?}");
    }
    assert_eq!(f.luma_at(0, 0), y);
}

#[test]
fn a_short_slice_is_refused_with_the_size_it_needs() {
    let e = FrameRef::packed(4, 4, PixelFormat::Nv12, &[0; 10]).unwrap_err();
    assert_eq!(e.needed, 24);
    assert!(e.to_string().contains("needs 24 bytes"));
}

#[test]
fn downscale_picks_the_middle_of_each_cell() {
    let px: Vec<u8> = (0..16).collect();
    let f = FrameRef::packed(4, 4, PixelFormat::Gray8, &px).unwrap();
    let mut out = [0u8; 4];
    f.downscale_luma(&mut out, 2, 2);
    assert_eq!(out, [5, 7, 13, 15]);
}
