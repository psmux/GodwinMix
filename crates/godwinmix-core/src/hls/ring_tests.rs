use super::*;

const SEG: u64 = 2_000_000_000;
const PART: u64 = 333_333_333;

fn part(tag: u8, independent: bool) -> Part {
    Part { bytes: Bytes::from(vec![tag; 100]), duration_ns: PART, independent }
}

/// A ring with one init and `n` whole segments of six parts from time zero.
fn filled(capacity: usize, n: u64) -> Ring {
    let mut r = Ring::new(capacity);
    r.set_init(Bytes::from_static(b"init"));
    for i in 0..n {
        r.begin(i * SEG, SEG, 0).unwrap();
        for p in 0..6 {
            assert!(r.push_part(part(i as u8, p == 0)));
        }
    }
    r.close();
    r
}

#[test]
fn nothing_begins_before_an_init() {
    let mut r = Ring::new(5);
    assert_eq!(r.begin(0, SEG, 0), None);
    assert!(!r.push_part(part(0, true)));
}

#[test]
fn numbers_come_from_time_so_rungs_agree() {
    let mut a = Ring::new(5);
    a.set_init(Bytes::from_static(b"a"));
    let mut b = Ring::new(5);
    b.set_init(Bytes::from_static(b"b"));
    // Rung b joins three segments later, a few milliseconds off the boundary.
    assert_eq!(a.begin(0, SEG, 0), Some(0));
    assert_eq!(a.begin(SEG, SEG, 0), Some(1));
    assert_eq!(a.begin(2 * SEG, SEG, 0), Some(2));
    assert_eq!(a.begin(3 * SEG, SEG, 0), Some(3));
    assert_eq!(b.begin(3 * SEG + 21_000_000, SEG, 0), Some(3));
    // After the first, numbers are consecutive whatever a segment's length.
    assert_eq!(b.begin(7 * SEG, SEG, 0), Some(4));
}

#[test]
fn the_ring_keeps_only_its_capacity() {
    let r = filled(4, 10);
    let v = r.view();
    assert_eq!(v.segments.iter().map(|s| s.msn).collect::<Vec<_>>(), vec![6, 7, 8, 9]);
    assert!(r.segment(5).is_none());
    assert_eq!(r.segment(9).unwrap().len(), 6);
    assert_eq!(r.memory(), 4 * 6 * 100 + 4);
}

#[test]
fn segments_are_the_same_memory_as_their_parts() {
    let r = filled(4, 2);
    let whole = r.segment(1).unwrap();
    let first = r.part(1, 0).unwrap();
    // Handed out by reference: the same allocation, not a copy.
    assert_eq!(whole[0].as_ptr(), first.as_ptr());
}

#[test]
fn an_open_segment_is_not_served_whole_but_its_parts_are() {
    let mut r = filled(4, 1);
    r.begin(SEG, SEG, 0);
    r.push_part(part(9, true));
    assert!(r.segment(1).is_none());
    assert!(r.part(1, 0).is_some());
    assert!(r.part(1, 1).is_none());
    assert_eq!(r.position(), Position { complete: Some(0), open: Some((1, 1)) });
}

#[test]
fn position_answers_blocking_reload() {
    let pos = Position { complete: Some(4), open: Some((5, 2)) };
    assert!(pos.reached(4, None));
    assert!(!pos.reached(5, None));
    assert!(pos.reached(5, Some(1)));
    assert!(!pos.reached(5, Some(2)));
    assert!(!pos.reached(6, Some(0)));
    assert!(pos.reached(3, Some(9)));
    assert_eq!(pos.newest(), Some(5));
}

#[test]
fn a_new_init_is_kept_while_segments_use_the_old_one() {
    let mut r = filled(3, 2);
    assert_eq!(r.set_init(Bytes::from_static(b"init")), 0, "the same header twice is one init");
    assert_eq!(r.set_init(Bytes::from_static(b"second")), 1);
    r.begin(2 * SEG, SEG, 0);
    r.push_part(part(2, true));
    assert!(r.init(0).is_some(), "segments 0 and 1 still need init 0");
    r.begin(3 * SEG, SEG, 0);
    r.begin(4 * SEG, SEG, 0);
    assert!(r.init(0).is_none(), "nothing left refers to init 0");
    assert_eq!(r.view().segments.last().unwrap().init, 1);
}

#[test]
fn versions_move_with_every_change() {
    let mut r = filled(4, 1);
    let v = r.version();
    r.begin(SEG, SEG, 0);
    assert!(r.version() > v);
    let v = r.version();
    r.push_part(part(1, true));
    assert!(r.version() > v);
}
