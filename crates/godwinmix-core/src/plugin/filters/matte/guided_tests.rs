use super::*;
fn apply(mask: &[u8], guide: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    alpha(mask, (16, 8), (guide, 128), (128, 64), Region { x: 0, y: 0, w: 128, h: 64 },
        &std::array::from_fn(|i| i as u8), &mut out);
    out
}
#[test]
fn flat_mask_stays_flat_with_textured_guide() {
    let guide: Vec<_> = (0..128 * 64).map(|i| (i * 37 % 256) as u8).collect();
    let out = apply(&[96; 128], &guide);
    assert!(out.iter().all(|&v| v.abs_diff(96) <= 1));
}
#[test]
fn known_edge_follows_guide_instead_of_model_grid() {
    let guide: Vec<_> = (0..128 * 64).map(|i| if i % 128 < 66 { 24 } else { 224 }).collect();
    let mask: Vec<_> = (0..128).map(|i| if i % 16 < 8 { 0 } else { 255 }).collect();
    let out = apply(&mask, &guide);
    let row = &out[16 * 64..][..64];
    assert!(row[32] < 55, "background {}", row[32]);
    assert!(row[33] > 200, "foreground {}", row[33]);
    assert!(row[0] < 2 && row[63] > 253);
}
