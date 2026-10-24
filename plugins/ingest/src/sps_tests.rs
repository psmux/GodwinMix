//! Reading the SPS: the bits, the size, and a short one.

use super::*;

#[test]
fn exponential_golomb_reads_the_textbook_values() {
    // 1, 010, 011, 00100 are 0, 1, 2 and 3.
    let mut b = Bits { data: vec![0b1010_0110, 0b0100_0000], at: 0 };
    assert_eq!(b.ue(), Some(0));
    assert_eq!(b.ue(), Some(1));
    assert_eq!(b.ue(), Some(2));
    assert_eq!(b.ue(), Some(3));
}

#[test]
fn an_emulation_prevention_byte_is_not_part_of_the_payload() {
    let b = Bits::new(&[0x00, 0x00, 0x03, 0x01]);
    assert_eq!(b.data, vec![0x00, 0x00, 0x01]);
}

#[test]
fn a_1080p_sps_is_cropped_from_1088_to_1080() {
    // What x264 writes for 1920x1080: 68 macroblock rows, cropped by 8.
    let sps = [
        0x67, 0x64, 0x00, 0x28, 0xac, 0xd9, 0x40, 0x78, 0x02, 0x27, 0xe5, 0xc0, 0x44, 0x00,
        0x00, 0x03, 0x00, 0x04, 0x00, 0x00, 0x03, 0x00, 0xf0, 0x3c, 0x60, 0xc6, 0x58,
    ];
    assert_eq!(size(&sps), Some((1920, 1080)));
}

#[test]
fn a_truncated_sps_is_nothing_rather_than_a_panic() {
    assert_eq!(size(&[0x67, 0x64]), None);
    assert_eq!(size(&[]), None);
}
