//! The session's rules, one datagram at a time, against packets written out
//! as a switcher sends them.

use std::time::{Duration, Instant};

const RESEND_AFTER: Duration = Duration::from_millis(200);
const SILENT_AFTER: Duration = Duration::from_secs(5);

use super::*;
use crate::wire::block;

const MAX_RESENDS: u32 = 10;
const FULL_BACKOFF: Duration = Duration::from_secs(5);

pub(super) const SESSION: u16 = 0x1234;
const REAL: u16 = 0x8123;

pub(super) fn switcher_hello(accept: u8) -> Vec<u8> {
    vec![0x10, 0x14, 0x12, 0x34, 0, 0, 0, 0, 0, 0, 0, 0, accept, 0, 0, 0, 0, 0, 0, 0]
}

fn reliable(id: u16, payload: &[u8]) -> Vec<u8> {
    Header { flags: Flags::ACK_REQUEST, session: REAL, packet_id: id, ..Header::default() }.packet(payload)
}

pub(super) fn drain(session: &mut Session) -> Vec<Vec<u8>> {
    std::iter::from_fn(|| session.poll_transmit()).collect()
}

/// A session that has said hello and been answered, and has had the dump.
pub(super) fn opened(now: Instant) -> Session {
    let mut s = Session::new(SESSION, now);
    s.handle_timeout(now);
    s.handle_datagram(&switcher_hello(2), now);
    s.handle_datagram(&reliable(1, &block(b"InCm", &[1, 0, 0, 0])), now);
    drain(&mut s);
    s
}

#[test]
fn the_hello_and_its_ack_have_the_expected_bytes() {
    let now = Instant::now();
    let mut s = Session::new(SESSION, now);
    s.handle_timeout(now);
    assert_eq!(
        drain(&mut s),
        [vec![0x10, 0x14, 0x12, 0x34, 0, 0, 0, 0, 0, 0x3a, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]]
    );
    s.handle_datagram(&switcher_hello(2), now);
    assert_eq!(drain(&mut s), [vec![0x80, 0x0c, 0x12, 0x34, 0, 0, 0, 0, 0, 0, 0, 0]]);
    assert_eq!(s.link().status, LinkStatus::Syncing);
}

#[test]
fn the_link_is_up_after_init_complete_and_each_packet_is_acked() {
    let now = Instant::now();
    let mut s = Session::new(SESSION, now);
    s.handle_timeout(now);
    s.handle_datagram(&switcher_hello(2), now);
    drain(&mut s);
    s.handle_datagram(&reliable(1, &block(b"PrgI", &[0, 0, 0, 3])), now);
    assert_eq!(drain(&mut s), [vec![0x80, 0x0c, 0x81, 0x23, 0, 1, 0, 0, 0, 0, 0, 0]]);
    assert_eq!(s.link().status, LinkStatus::Syncing);
    s.handle_datagram(&reliable(2, &block(b"InCm", &[1, 0, 0, 0])), now);
    assert!(s.link().is_up());
    assert_eq!(s.state().program(0), Some(3));
}

#[test]
fn a_packet_after_a_gap_waits_and_a_repeat_is_acked_again() {
    let now = Instant::now();
    let mut s = opened(now);
    s.handle_datagram(&reliable(3, &block(b"PrgI", &[0, 0, 0, 9])), now);
    assert!(drain(&mut s).is_empty(), "packet 3 before 2 is not taken");
    assert_eq!(s.state().program(0), None);
    s.handle_datagram(&reliable(1, &[]), now);
    assert_eq!(drain(&mut s)[0][4..6], [0, 1], "a repeat of 1 is acked as 1");
}

#[test]
fn commands_go_out_reliably_and_are_resent_until_acked() {
    let start = Instant::now();
    let mut s = opened(start);
    s.send(&[Command::Cut { me: 0 }], start).unwrap();
    let sent = drain(&mut s);
    let mut expected = vec![0x08, 0x18, 0x81, 0x23, 0, 0, 0, 0, 0, 0, 0, 1];
    expected.extend(Command::Cut { me: 0 }.encode());
    assert_eq!(sent, [expected.clone()]);
    s.handle_timeout(start + RESEND_AFTER);
    let again = drain(&mut s);
    expected[0] = 0x28;
    assert_eq!(again, [expected]);
    let ack = Header { flags: Flags::ACK_REPLY, session: REAL, ack_id: 1, ..Header::default() };
    s.handle_datagram(&ack.packet(&[]), start + RESEND_AFTER);
    s.handle_timeout(start + RESEND_AFTER * 3);
    assert!(drain(&mut s).is_empty());
}

#[test]
fn a_command_while_down_is_refused_with_the_link() {
    let now = Instant::now();
    let mut s = Session::new(SESSION, now);
    let refused = s.send(&[Command::Cut { me: 0 }], now).unwrap_err();
    assert_eq!(refused.0.status, LinkStatus::Connecting);
}

#[test]
fn a_full_switcher_is_retried_after_a_wait() {
    let start = Instant::now();
    let mut s = Session::new(SESSION, start);
    s.handle_timeout(start);
    s.handle_datagram(&switcher_hello(3), start);
    drain(&mut s);
    assert_eq!(s.link().reason.as_deref(), Some(reasons::FULL));
    assert_eq!(s.poll_timeout(), start + FULL_BACKOFF);
    s.handle_timeout(start + FULL_BACKOFF);
    assert_eq!(drain(&mut s).len(), 1);
}

#[test]
fn silence_ends_the_session_and_a_new_one_counts() {
    let start = Instant::now();
    let mut s = opened(start);
    s.handle_timeout(start + SILENT_AFTER);
    assert_eq!(s.link().reason.as_deref(), Some(reasons::SILENT));
    assert_eq!(s.link().status, LinkStatus::Connecting);
    let later = start + SILENT_AFTER;
    s.handle_timeout(later);
    s.handle_datagram(&switcher_hello(2), later);
    assert_eq!(s.link().sessions, 2);
    assert_eq!(s.link().reason, None);
}

#[test]
fn ten_unanswered_resends_end_the_session() {
    let start = Instant::now();
    let mut s = opened(start);
    s.send(&[Command::Cut { me: 0 }], start).unwrap();
    let mut now = start;
    for _ in 0..=MAX_RESENDS {
        now += RESEND_AFTER;
        // The switcher still pings, so it is not silence that ends it.
        s.handle_datagram(&Header { flags: Flags::ACK_REPLY, session: REAL, ..Header::default() }.packet(&[]), now);
        s.handle_timeout(now);
    }
    assert_eq!(s.link().reason.as_deref(), Some(reasons::UNACKED));
}
