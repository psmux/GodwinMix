//! The send window, acknowledgements and resends.

use super::*;

const RESEND_AFTER: Duration = Duration::from_millis(200);
const MAX_RESENDS: u32 = 10;

#[test]
fn covered_counts_across_the_wrap() {
    assert!(covered(5, 5));
    assert!(covered(5, 1));
    assert!(!covered(5, 6));
    assert!(covered(2, 0x7ffe));
    assert!(!covered(0x7ffe, 2));
}

#[test]
fn an_ack_clears_everything_up_to_it() {
    let now = Instant::now();
    let mut inflight = Inflight::default();
    for id in [0x7ffe, 0x7fff, 0, 1] {
        inflight.push(id, vec![0x08, 12], now);
    }
    inflight.ack(0, now);
    assert_eq!(inflight.len(), 1);
}

#[test]
fn no_more_than_the_window_is_out_at_once() {
    let now = Instant::now();
    let mut inflight = Inflight::default();
    let sent: Vec<bool> = (1..=WINDOW as u16 + 3).map(|id| inflight.push(id, vec![0x08, 12, id as u8], now).is_some()).collect();
    assert_eq!(sent.iter().filter(|s| **s).count(), WINDOW, "the rest wait");
    let next = inflight.ack(2, now);
    assert_eq!(next.iter().map(|b| b[2]).collect::<Vec<_>>(), [9, 10], "two acknowledged, two more go, in order");
    assert_eq!(inflight.len(), WINDOW);
}

#[test]
fn a_packet_is_resent_as_a_retransmit_and_then_given_up_on() {
    let start = Instant::now();
    let mut inflight = Inflight::default();
    inflight.push(1, vec![0x08, 12], start);
    let mut now = start;
    for _ in 0..MAX_RESENDS {
        now += RESEND_AFTER;
        let again = inflight.due(now, RESEND_AFTER, MAX_RESENDS).unwrap();
        assert_eq!(again[0][0], 0x28);
    }
    assert!(inflight.due(now + RESEND_AFTER, RESEND_AFTER, MAX_RESENDS).is_err());
}
