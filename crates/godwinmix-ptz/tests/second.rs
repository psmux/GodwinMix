//! `Ptz::second`: the HTTP drivers and the fake give a second driver on the
//! same camera that works while the first is in use; VISCA gives none.

use std::time::Duration;

use godwinmix_ptz::fake::{self, visca::Options};
use godwinmix_ptz::{open, Protocol, Target, Velocity};

fn target(protocol: Protocol, address: String) -> Target {
    let mut t = Target::new(protocol, &address);
    t.timeout = Duration::from_millis(500);
    t
}

fn moves_through_both(t: Target, head: fake::Shared) {
    let mut first = open(&t).unwrap();
    let mut second = first.second().expect("this protocol takes a second connection");
    first.move_at(Velocity::new(0.5, 0.0, 0.0)).unwrap();
    second.save_preset(4).unwrap();
    assert!(second.position().is_ok());
    assert!(head.lock().unwrap().presets.contains_key(&4));
    first.stop().unwrap();
    assert_eq!(head.lock().unwrap().velocity, Velocity::STOP);
}

#[test]
fn onvif_canon_and_the_fake_give_a_second_driver() {
    let head = fake::shared();
    let mut t = target(Protocol::Onvif, fake::onvif::serve_onvif(head.clone()));
    (t.user, t.password) = (fake::onvif::USER.into(), fake::onvif::PASSWORD.into());
    moves_through_both(t, head);
    let head = fake::shared();
    let mut t = target(Protocol::Canon, fake::canon::serve_canon(head.clone()).to_string());
    (t.user, t.password) = (fake::canon::USER.into(), fake::canon::PASSWORD.into());
    moves_through_both(t, head);
    let cam = open(&Target::new(Protocol::Fake, "fake")).unwrap();
    assert!(cam.second().is_some());
}

#[test]
fn visca_keeps_one_connection() {
    let ip = fake::visca::serve_ip(fake::shared(), Options::default());
    assert!(open(&target(Protocol::ViscaIp, ip.to_string())).unwrap().second().is_none());
    let tcp = fake::visca::serve_tcp(fake::shared());
    assert!(open(&target(Protocol::ViscaTcp, tcp.to_string())).unwrap().second().is_none());
}
