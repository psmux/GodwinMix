//! Every driver against its fake camera, through the trait, over loopback:
//! the same checks for VISCA over IP, raw VISCA over TCP, ONVIF and Canon.
//! The fakes keep a moving head, so a wrong speed, sign or unit shows up as
//! the camera being in the wrong place.

use std::thread::sleep;
use std::time::Duration;

use godwinmix_ptz::fake::{self, visca::Options, Shared};
use godwinmix_ptz::{open, Code, Exposure, ExposureMode, Focus, Position, Protocol, Ptz, Target, Velocity};

fn near(a: f32, b: f32, within: f32) -> bool {
    (a - b).abs() <= within
}

fn exercise(cam: &mut dyn Ptz, head: &Shared) {
    // Right and up at half speed for a moment: the head moves that way.
    cam.move_at(Velocity::new(0.5, 0.5, 0.0)).unwrap();
    sleep(Duration::from_millis(300));
    cam.stop().unwrap();
    let p = cam.position().unwrap();
    assert!(p.pan > 0.08 && p.pan < 0.4, "pan {p:?}");
    assert!(p.tilt > 0.08 && p.tilt < 0.4, "tilt {p:?}");
    sleep(Duration::from_millis(100));
    let still = cam.position().unwrap();
    assert!(near(still.pan, p.pan, 0.01), "stopped means stopped: {p:?} then {still:?}");

    // A preset brings it back after an absolute move away.
    cam.save_preset(3).unwrap();
    cam.goto(Position { pan: -0.5, tilt: -0.25, zoom: 0.5, raw: [0; 3] }).unwrap();
    let away = cam.position().unwrap();
    assert!(near(away.pan, -0.5, 0.01) && near(away.tilt, -0.25, 0.01) && near(away.zoom, 0.5, 0.01), "{away:?}");
    cam.goto_preset(3).unwrap();
    let back = cam.position().unwrap();
    assert!(near(back.pan, p.pan, 0.01) && near(back.tilt, p.tilt, 0.01), "{back:?} after recalling {p:?}");

    // Zoom tighter.
    cam.zoom(1.0).unwrap();
    sleep(Duration::from_millis(200));
    cam.zoom(0.0).unwrap();
    assert!(cam.position().unwrap().zoom > back.zoom + 0.03);

    // Focus and exposure land on the head.
    cam.focus(Focus::Manual).unwrap();
    assert!(!head.lock().unwrap().focus_auto);
    cam.focus(Focus::Auto).unwrap();
    assert!(head.lock().unwrap().focus_auto);
    cam.exposure(Exposure { mode: Some(ExposureMode::Manual), shutter: Some(250), ..Exposure::default() }).unwrap();
    let e = head.lock().unwrap().exposure;
    assert_eq!((e.mode, e.shutter), (Some(ExposureMode::Manual), Some(250)));

    assert_eq!(cam.goto_preset(0).unwrap_err().code, Code::BadArgument);
}

fn target(protocol: Protocol, address: String) -> Target {
    let mut t = Target::new(protocol, &address);
    t.timeout = Duration::from_millis(500);
    t
}

#[test]
fn visca_over_ip_drives_the_fake_camera() {
    let head = fake::shared();
    let at = fake::visca::serve_ip(head.clone(), Options { want_reset: true, ..Options::default() });
    exercise(open(&target(Protocol::ViscaIp, at.to_string())).unwrap().as_mut(), &head);
}

#[test]
fn visca_over_tcp_drives_the_fake_camera() {
    let head = fake::shared();
    let at = fake::visca::serve_tcp(head.clone());
    exercise(open(&target(Protocol::ViscaTcp, at.to_string())).unwrap().as_mut(), &head);
}

#[test]
fn onvif_drives_the_fake_camera() {
    let head = fake::shared();
    let mut t = target(Protocol::Onvif, fake::onvif::serve_onvif(head.clone()));
    (t.user, t.password) = (fake::onvif::USER.into(), fake::onvif::PASSWORD.into());
    exercise(open(&t).unwrap().as_mut(), &head);
}

#[test]
fn onvif_with_the_wrong_password_says_so() {
    let mut t = target(Protocol::Onvif, fake::onvif::serve_onvif(fake::shared()));
    (t.user, t.password) = (fake::onvif::USER.into(), "wrong".into());
    let e = open(&t).err().expect("refused");
    assert_eq!(e.code, Code::Unauthorized);
    assert!(e.message.contains("clock"), "{e}");
}

#[test]
fn canon_drives_the_fake_camera_through_a_digest_login() {
    let head = fake::shared();
    let mut t = target(Protocol::Canon, fake::canon::serve_canon(head.clone()).to_string());
    (t.user, t.password) = (fake::canon::USER.into(), fake::canon::PASSWORD.into());
    exercise(open(&t).unwrap().as_mut(), &head);
}

#[test]
fn canon_focus_drive_in_auto_focus_is_refused_with_the_reason() {
    let head = fake::shared();
    let mut t = target(Protocol::Canon, fake::canon::serve_canon(head).to_string());
    (t.user, t.password) = (fake::canon::USER.into(), fake::canon::PASSWORD.into());
    let mut cam = open(&t).unwrap();
    let e = cam.focus(Focus::Drive(0.5)).unwrap_err();
    assert_eq!(e.code, Code::Refused);
    assert!(e.message.contains("manual focus"), "{e}");
    cam.focus(Focus::Manual).unwrap();
    cam.focus(Focus::Drive(0.5)).unwrap();
}

#[test]
fn canon_without_a_login_is_told_where_to_set_one() {
    let mut cam = open(&target(Protocol::Canon, fake::canon::serve_canon(fake::shared()).to_string())).unwrap();
    let e = cam.stop().unwrap_err();
    assert_eq!(e.code, Code::Unauthorized);
}

#[test]
fn the_fake_protocol_needs_no_camera() {
    let mut cam = open(&target(Protocol::Fake, String::new())).unwrap();
    cam.move_at(Velocity::new(1.0, 0.0, 0.0)).unwrap();
    sleep(Duration::from_millis(50));
    assert!(cam.position().unwrap().pan > 0.0);
}

#[test]
fn an_unknown_protocol_lists_the_known_ones() {
    let e = Protocol::parse("pelco").unwrap_err();
    assert!(e.message.contains("visca-ip, visca-tcp, onvif, canon, fake"), "{e}");
}
