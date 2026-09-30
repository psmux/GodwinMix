use super::*;

#[test]
fn nv12_1080p_is_two_planes_with_aligned_rows() {
    let l = Layout::new(Format::Nv12, 1920, 1080).unwrap();
    assert_eq!(l.n_planes, 2);
    assert_eq!(l.strides[..2], [1920, 1920]);
    assert_eq!(l.offsets[1], 1920 * 1080);
    assert_eq!(l.size, 1920 * 1080 * 3 / 2);
    assert_eq!(l.row_bytes(1), 1920);
    assert_eq!(l.rows(1), 540);
}

#[test]
fn i420_odd_size_rounds_chroma_up() {
    let l = Layout::new(Format::I420, 641, 361).unwrap();
    assert_eq!(l.n_planes, 3);
    assert_eq!(l.row_bytes(1), 321);
    assert_eq!(l.rows(2), 181);
    assert_eq!(l.strides[0] as usize % ROW_ALIGN, 0);
    assert_eq!(l.size, l.offsets[2] + 181 * l.strides[2] as u64);
}

#[test]
fn names_round_trip_and_zero_size_is_refused() {
    for f in Format::ALL {
        assert_eq!(Format::from_name(f.name()), Some(f));
        assert_eq!(Format::from_code(f as u32), Some(f));
    }
    assert!(Layout::new(Format::Nv12, 0, 10).is_err());
}
