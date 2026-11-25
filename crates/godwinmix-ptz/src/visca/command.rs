//! VISCA command and inquiry bytes. Every function answers one message,
//! header to terminator, for camera `addr` (1 to 7). The byte layouts are
//! the ones in Sony's VISCA command lists (BRC and SRG series) and match what
//! PTZOptics, Lumens, Marshall and most church PTZ cameras accept.

pub use super::ranges::Ranges;
use crate::types::{clamp1, ExposureMode, DEAD};

fn head(addr: u8) -> u8 {
    0x80 | (addr & 0x07)
}

/// `8x 01 06 01 VV WW 0Y 0Z FF`: pan and tilt at a speed, or stop.
pub fn pan_tilt(addr: u8, pan: f32, tilt: f32, r: &Ranges) -> Vec<u8> {
    let (vv, x) = axis(pan, r.pan_speed, 0x02, 0x01);
    let (ww, y) = axis(tilt, r.tilt_speed, 0x01, 0x02);
    vec![head(addr), 0x01, 0x06, 0x01, vv, ww, x, y, 0xFF]
}

/// The speed byte and the direction byte for one axis. A stopped axis still
/// carries a valid speed of 1, which some cameras insist on.
fn axis(v: f32, max: u8, positive: u8, negative: u8) -> (u8, u8) {
    let v = clamp1(v);
    if v.abs() < DEAD {
        return (0x01, 0x03);
    }
    let speed = ((v.abs() * max as f32).ceil() as u8).clamp(1, max);
    (speed, if v > 0.0 { positive } else { negative })
}

/// `8x 01 04 07 2p FF` tighter, `3p` wider, `00` stop. p is 0 to 7.
pub fn zoom(addr: u8, speed: f32) -> Vec<u8> {
    vec![head(addr), 0x01, 0x04, 0x07, drive(speed, 0x20, 0x30), 0xFF]
}

/// `8x 01 04 08 2p FF` far, `3p` near, `00` stop.
pub fn focus_drive(addr: u8, speed: f32) -> Vec<u8> {
    vec![head(addr), 0x01, 0x04, 0x08, drive(speed, 0x20, 0x30), 0xFF]
}

fn drive(v: f32, positive: u8, negative: u8) -> u8 {
    let v = clamp1(v);
    if v.abs() < DEAD {
        return 0x00;
    }
    let p = ((v.abs() * 8.0).ceil() as u8).clamp(1, 8) - 1;
    (if v > 0.0 { positive } else { negative }) | p
}

/// `8x 01 04 38 02 FF` auto focus, `03` manual.
pub fn focus_auto(addr: u8, auto: bool) -> Vec<u8> {
    vec![head(addr), 0x01, 0x04, 0x38, if auto { 0x02 } else { 0x03 }, 0xFF]
}

/// `8x 01 04 18 01 FF`: focus once, then stay manual.
pub fn focus_one_push(addr: u8) -> Vec<u8> {
    vec![head(addr), 0x01, 0x04, 0x18, 0x01, 0xFF]
}

/// `8x 01 04 3F 0a pp FF`: a = 0 reset, 1 set, 2 recall. Preset n is pp n-1.
pub fn preset(addr: u8, action: u8, n: u16) -> Vec<u8> {
    vec![head(addr), 0x01, 0x04, 0x3F, action, (n.saturating_sub(1) & 0xFF) as u8, 0xFF]
}

/// `8x 01 06 02 VV WW 0Y 0Y 0Y 0Y 0Z 0Z 0Z 0Z FF`: go to a pan and tilt.
pub fn pan_tilt_absolute(addr: u8, pan: i32, tilt: i32, r: &Ranges) -> Vec<u8> {
    let mut m = vec![head(addr), 0x01, 0x06, 0x02, r.pan_speed, r.tilt_speed];
    m.extend(nibbles(pan as i16 as u16));
    m.extend(nibbles(tilt as i16 as u16));
    m.push(0xFF);
    m
}

/// `8x 01 04 47 0p 0q 0r 0s FF`: go to a zoom position.
pub fn zoom_absolute(addr: u8, zoom: u16) -> Vec<u8> {
    let mut m = vec![head(addr), 0x01, 0x04, 0x47];
    m.extend(nibbles(zoom));
    m.push(0xFF);
    m
}

/// A 16 bit value as four bytes of one nibble each, high first.
pub fn nibbles(v: u16) -> [u8; 4] {
    [(v >> 12) as u8 & 0xF, (v >> 8) as u8 & 0xF, (v >> 4) as u8 & 0xF, v as u8 & 0xF]
}

/// `8x 01 04 39 0p FF`: the exposure mode.
pub fn exposure_mode(addr: u8, mode: ExposureMode) -> Vec<u8> {
    let p = match mode {
        ExposureMode::Auto => 0x00,
        ExposureMode::Manual => 0x03,
        ExposureMode::ShutterPriority => 0x0A,
        ExposureMode::IrisPriority => 0x0B,
    };
    vec![head(addr), 0x01, 0x04, 0x39, p, 0xFF]
}

/// `8x 01 04 4B 00 00 0p 0q FF` iris, `4A` shutter, `4C` gain: a direct value.
pub fn exposure_direct(addr: u8, which: u8, value: u16) -> Vec<u8> {
    let n = nibbles(value);
    vec![head(addr), 0x01, 0x04, which, 0x00, 0x00, n[2], n[3], 0xFF]
}

pub const IRIS: u8 = 0x4B;
pub const SHUTTER: u8 = 0x4A;
pub const GAIN: u8 = 0x4C;

/// `8x 09 06 12 FF`: where pan and tilt are.
pub fn inquire_pan_tilt(addr: u8) -> Vec<u8> {
    vec![head(addr), 0x09, 0x06, 0x12, 0xFF]
}

/// `8x 09 04 47 FF`: where zoom is.
pub fn inquire_zoom(addr: u8) -> Vec<u8> {
    vec![head(addr), 0x09, 0x04, 0x47, 0xFF]
}
