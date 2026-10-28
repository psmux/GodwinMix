use super::*;

/// A 1920x1080 29.97 interlaced sequence header and extension, then a GOP
/// header and the start of a picture.
fn keyframe(progressive: bool) -> Vec<u8> {
    let mut v = vec![0, 0, 1, 0xb3, 0x78, 0x04, 0x38, 0x34, 0xff, 0xff, 0xe0, 0x18];
    v.extend_from_slice(&[0, 0, 1, 0xb5, 0x14, if progressive { 0x8a } else { 0x82 }, 0x00, 0x01, 0x00, 0x00]);
    v.extend_from_slice(&[0, 0, 1, 0xb8, 0, 8, 0, 0]);
    v.extend_from_slice(&[0, 0, 1, 0x00, 0x00, 0x0f, 0xff, 0xf8]);
    v
}

#[test]
fn a_picture_after_a_sequence_header_is_a_keyframe_and_says_its_shape() {
    let key = keyframe(false);
    let s = sequence(&key).unwrap();
    assert_eq!(s, Sequence { width: 1920, height: 1080, fps: (30000, 1001), progressive: false, par: (1, 1) });
    assert!(sequence(&keyframe(true)).unwrap().progressive);
    let other = [0, 0, 1, 0x00, 0x00, 0x57, 0xff, 0xf8, 0, 0, 1, 0x01, 0x22];
    assert!(sequence(&other).is_none() && !starts_gop(&other));
}

#[test]
fn a_tag_carries_the_picture_after_eight_bytes_with_its_offset() {
    let key = keyframe(false);
    let t = tag(&key, 1000, -33);
    assert!(t.keyframe && !t.sequence_header && t.track == 0);
    assert_eq!(t.payload[0], 0x91);
    assert!(is(&t.payload));
    assert_eq!(crate::eflv::frame(&t.payload), Some((PREFIX, -33)));
    assert_eq!(&t.payload[PREFIX..], key.as_slice());
    assert_eq!(tag(&[0, 0, 1, 0, 0, 0x57], 0, 0).payload[0], 0xa1);
}

/// A 704x480 29.97 sequence header with the aspect code given, and an
/// interlaced extension.
fn sd(aspect: u8) -> Vec<u8> {
    let mut v = vec![0, 0, 1, 0xb3, 0x2c, 0x01, 0xe0, (aspect << 4) | 4, 0xff, 0xff, 0xe0, 0x18];
    v.extend_from_slice(&[0, 0, 1, 0xb5, 0x14, 0x82, 0x00, 0x01, 0x00, 0x00]);
    v
}

#[test]
fn the_aspect_code_gives_the_shape_of_a_stored_pixel() {
    let wide = sequence(&sd(3)).unwrap();
    assert_eq!((wide.width, wide.height, wide.par), (704, 480, (40, 33)));
    assert_eq!(sequence(&sd(2)).unwrap().par, (10, 11));
    assert_eq!(sequence(&sd(1)).unwrap().par, (1, 1));
    // A code this does not know is taken as square, never refused.
    assert_eq!(sequence(&sd(9)).unwrap().par, (1, 1));
}
