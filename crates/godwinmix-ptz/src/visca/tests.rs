//! The bytes against Sony's command list, written out as literals, and the
//! two links against the fake camera with datagrams lost.

use std::time::Duration;

use super::command::*;
use super::reply::{from_nibbles, parse, split, Reply};
use super::*;
use crate::fake::{self, visca::Options};
use crate::types::ExposureMode;

#[test]
fn every_command_is_the_bytes_the_command_list_gives() {
    let r = Ranges::default();
    assert_eq!(pan_tilt(1, 1.0, 0.0, &r), [0x81, 0x01, 0x06, 0x01, 0x18, 0x01, 0x02, 0x03, 0xFF]);
    assert_eq!(pan_tilt(1, -0.5, 1.0, &r), [0x81, 0x01, 0x06, 0x01, 0x0C, 0x14, 0x01, 0x01, 0xFF]);
    assert_eq!(pan_tilt(1, 0.0, -1.0, &r), [0x81, 0x01, 0x06, 0x01, 0x01, 0x14, 0x03, 0x02, 0xFF]);
    assert_eq!(pan_tilt(2, 0.0, 0.0, &r), [0x82, 0x01, 0x06, 0x01, 0x01, 0x01, 0x03, 0x03, 0xFF]);
    assert_eq!(zoom(1, 1.0), [0x81, 0x01, 0x04, 0x07, 0x27, 0xFF]);
    assert_eq!(zoom(1, -0.1), [0x81, 0x01, 0x04, 0x07, 0x30, 0xFF]);
    assert_eq!(zoom(1, 0.0), [0x81, 0x01, 0x04, 0x07, 0x00, 0xFF]);
    assert_eq!(focus_drive(1, -1.0), [0x81, 0x01, 0x04, 0x08, 0x37, 0xFF]);
    assert_eq!(focus_auto(1, true), [0x81, 0x01, 0x04, 0x38, 0x02, 0xFF]);
    assert_eq!(focus_auto(1, false), [0x81, 0x01, 0x04, 0x38, 0x03, 0xFF]);
    assert_eq!(focus_one_push(1), [0x81, 0x01, 0x04, 0x18, 0x01, 0xFF]);
    assert_eq!(preset(1, 0x02, 1), [0x81, 0x01, 0x04, 0x3F, 0x02, 0x00, 0xFF]);
    assert_eq!(preset(1, 0x01, 16), [0x81, 0x01, 0x04, 0x3F, 0x01, 0x0F, 0xFF]);
    assert_eq!(
        pan_tilt_absolute(1, -1, 0x0123, &r),
        [0x81, 0x01, 0x06, 0x02, 0x18, 0x14, 0x0F, 0x0F, 0x0F, 0x0F, 0x00, 0x01, 0x02, 0x03, 0xFF]
    );
    assert_eq!(zoom_absolute(1, 0x4000), [0x81, 0x01, 0x04, 0x47, 0x04, 0x00, 0x00, 0x00, 0xFF]);
    assert_eq!(exposure_mode(1, ExposureMode::Manual), [0x81, 0x01, 0x04, 0x39, 0x03, 0xFF]);
    assert_eq!(exposure_direct(1, IRIS, 0x0B), [0x81, 0x01, 0x04, 0x4B, 0x00, 0x00, 0x00, 0x0B, 0xFF]);
    assert_eq!(inquire_pan_tilt(1), [0x81, 0x09, 0x06, 0x12, 0xFF]);
    assert_eq!(inquire_zoom(1), [0x81, 0x09, 0x04, 0x47, 0xFF]);
}

#[test]
fn replies_read_as_ack_done_and_error() {
    assert_eq!(parse(&[0x90, 0x41, 0xFF]), Some(Reply::Ack));
    assert_eq!(parse(&[0x90, 0x51, 0xFF]), Some(Reply::Done(vec![])));
    assert_eq!(parse(&[0x90, 0x50, 0x01, 0x02, 0x03, 0x04, 0xFF]), Some(Reply::Done(vec![1, 2, 3, 4])));
    assert_eq!(parse(&[0x90, 0x61, 0x41, 0xFF]), Some(Reply::Error(0x41)));
    assert_eq!(parse(&[0x81, 0x01, 0xFF]), None);
    assert_eq!(from_nibbles(&[0x0F, 0x0F, 0x0F, 0x0F]), Some(0xFFFF));
    let (messages, rest) = split(&[0x90, 0x41, 0xFF, 0x90, 0x51, 0xFF, 0x90]);
    assert_eq!(messages.len(), 2);
    assert_eq!(rest, [0x90]);
}

#[test]
fn the_ip_header_carries_type_length_and_sequence() {
    let m = ip::wrap(ip::COMMAND, 0x0102_0304, &[0x81, 0x01, 0xFF]);
    assert_eq!(m, [0x01, 0x00, 0x00, 0x03, 0x01, 0x02, 0x03, 0x04, 0x81, 0x01, 0xFF]);
    assert_eq!(ip::unwrap(&m), Some((ip::COMMAND, 0x0102_0304, &[0x81, 0x01, 0xFF][..])));
    assert_eq!(ip::unwrap(&m[..10]), None);
}

fn quick() -> Duration {
    Duration::from_millis(150)
}

#[test]
fn a_lost_datagram_is_sent_again_and_a_camera_wanting_a_reset_gets_one() {
    let head = fake::shared();
    let at = fake::visca::serve_ip(head.clone(), Options { drop_every: 3, want_reset: true });
    let mut cam = Visca::over_ip(at, Ranges::default(), quick()).unwrap();
    for n in 1..=12u16 {
        cam.save_preset(n).unwrap();
    }
    assert_eq!(head.lock().unwrap().presets.len(), 12);
}

#[test]
fn a_camera_that_never_answers_times_out_with_the_port_in_the_message() {
    let silent = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut cam = Visca::over_ip(silent.local_addr().unwrap(), Ranges::default(), Duration::from_millis(50)).unwrap();
    let e = cam.stop().unwrap_err();
    assert_eq!(e.code, Code::Timeout);
    assert!(e.message.contains("52381"), "{e}");
}

#[test]
fn an_unknown_command_comes_back_as_a_syntax_error() {
    let head = fake::shared();
    let at = fake::visca::serve_tcp(head);
    let mut link = tcp::TcpLink::new(at, quick());
    let e = link.exchange(&[0x81, 0x01, 0x7E, 0x01, 0xFF], false).unwrap_err();
    assert_eq!(e.code, Code::Refused);
    assert!(e.message.contains("syntax"), "{e}");
}

#[test]
fn the_same_speed_twice_is_sent_once() {
    let head = fake::shared();
    let at = fake::visca::serve_tcp(head.clone());
    let mut cam = Visca::over_tcp(at, Ranges::default(), quick());
    for _ in 0..10 {
        cam.move_at(Velocity::new(0.5, 0.0, 0.0)).unwrap();
    }
    assert_eq!(head.lock().unwrap().moves, 2, "one pan and tilt, one zoom stop");
}
