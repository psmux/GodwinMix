//! Annex B and ADTS against hand written bytes.

use super::*;

#[test]
fn an_avcc_record_gives_its_parameter_sets_with_start_codes() {
    let record = [1, 0x64, 0, 0x1f, 0xff, 0xe1, 0, 2, 0x67, 0xaa, 1, 0, 2, 0x68, 0xbb];
    let c = VideoConfig::from_avcc(&record).unwrap();
    assert_eq!(c.length_size, 4);
    assert_eq!(c.parameter_sets, [0, 0, 0, 1, 0x67, 0xaa, 0, 0, 0, 1, 0x68, 0xbb]);
    let frame = [0, 0, 0, 2, 0x09, 0x10, 0, 0, 0, 3, 0x65, 1, 2];
    let (au, key) = c.annex_b(&frame, false);
    assert!(key, "an IDR slice starts a GOP whatever the tag said");
    assert_eq!(&au[..6], &[0, 0, 0, 1, 0x09, 0xf0], "our own delimiter, the frame's dropped");
    assert_eq!(&au[6..18], &c.parameter_sets[..]);
    assert_eq!(&au[18..], &[0, 0, 0, 1, 0x65, 1, 2]);
}

#[test]
fn an_adts_header_says_48k_stereo_lc_and_the_length() {
    // AAC LC, 48 kHz, stereo.
    let c = AudioConfig::from_asc(&[0x11, 0x90]).unwrap();
    assert_eq!(c.sample_rate(), 48_000);
    let h = c.adts(100);
    assert_eq!(&h[..2], &[0xff, 0xf1]);
    assert_eq!(h[2] >> 6, 1, "LC is profile 1");
    let len = (usize::from(h[3] & 3) << 11) | (usize::from(h[4]) << 3) | usize::from(h[5] >> 5);
    assert_eq!(len, 107);
    assert_eq!(((h[2] & 1) << 2) | (h[3] >> 6), 2, "two channels");
}
