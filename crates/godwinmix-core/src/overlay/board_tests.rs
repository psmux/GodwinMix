use super::*;

#[test]
fn a_crawl_carries_on_from_where_it_was_when_its_speed_changes() {
    let mut c = Clock::default();
    assert_eq!(c.travelled(1_000_000_000, 0, 100.0), 0.0, "starts at the edge");
    assert_eq!(c.travelled(2_000_000_000, 0, 100.0), 100.0);
    // Twice as fast from here: no jump, then twice the pace.
    assert_eq!(c.travelled(2_000_000_000, 0, 200.0), 100.0);
    assert_eq!(c.travelled(3_000_000_000, 0, 200.0), 300.0);
    // New words start again from the edge.
    assert_eq!(c.travelled(4_000_000_000, 1, 200.0), 0.0);
}
