//! The allocation answer kept and given again, on queries made by hand.

use super::*;

fn caps(width: i32) -> gst::Caps {
    gst::Caps::builder("video/x-raw").field("format", "I420").field("width", width).field("height", 180).build()
}

fn answered(width: i32) -> Option<Answer> {
    let mut q = gst::query::Allocation::new(Some(&caps(width)), true);
    q.add_allocation_pool(None::<&gst::BufferPool>, 86_400, 0, 0);
    q.add_allocation_meta::<gstreamer_video::VideoMeta>(None);
    kept(&q)
}

#[test]
fn a_repeat_for_the_same_caps_gets_the_same_answer_without_going_down() {
    let _ = gst::init();
    let memory = answered(320);
    let mut q = gst::query::Allocation::new(Some(&caps(320)), true);
    assert!(replay(&memory, q.get_mut().expect("writable")));
    let pools: Vec<_> = q.allocation_pools().map(|(p, s, mn, mx)| (p.is_some(), s, mn, mx)).collect();
    assert_eq!(pools, vec![(false, 86_400, 0, 0)]);
    assert!(q.find_allocation_meta::<gstreamer_video::VideoMeta>().is_some());
}

#[test]
fn new_caps_or_an_offered_pool_go_down_as_before() {
    let _ = gst::init();
    let memory = answered(320);
    let mut q = gst::query::Allocation::new(Some(&caps(640)), true);
    assert!(!replay(&memory, q.get_mut().expect("writable")), "other caps ask downstream");
    let mut with_pool = gst::query::Allocation::new(Some(&caps(320)), true);
    let pool = gst::BufferPool::new();
    with_pool.add_allocation_pool(Some(&pool), 86_400, 2, 0);
    assert!(kept(&with_pool).is_none(), "an answer with a pool object is never replayed");
    assert!(!replay(&None, q.get_mut().expect("writable")), "nothing remembered, nothing replayed");
}
