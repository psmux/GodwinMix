use super::*;

#[test]
fn near_blocks_join_and_far_ones_do_not() {
    let mut b = Blobs::default();
    b.start(10, 6);
    // A U shape whose arms meet only through the bottom row, a block one
    // gap away from it, and a lone block far off.
    for (x, y) in [(1, 1), (1, 2), (1, 3), (2, 3), (3, 3), (3, 2), (3, 1), (5, 3), (9, 0)] {
        for _ in 0..4 {
            b.count(x, y);
        }
    }
    b.label(4);
    let mut boxes = b.boxes().to_vec();
    boxes.sort_by_key(|s| s.x0);
    assert_eq!(boxes.len(), 2, "{boxes:?}");
    assert_eq!((boxes[0].x0, boxes[0].y0, boxes[0].x1, boxes[0].y1, boxes[0].blocks), (1, 1, 5, 3, 8));
    assert_eq!((boxes[1].x0, boxes[1].blocks), (9, 1));
}
