//! A fake VISCA camera on loopback, over VISCA over IP (UDP with Sony's
//! header) or raw TCP. It reads commands with its own decoder, written from
//! the command list rather than shared with the driver, so a wrong byte in
//! the driver shows up as a wrong position here.

mod serve;

pub use serve::{serve_ip, serve_tcp};

use super::Head;
use crate::types::{ExposureMode, Focus, Velocity};
use crate::visca::command::{nibbles, Ranges};

/// What the fake does to datagrams, for the loss tests.
#[derive(Debug, Default, Clone)]
pub struct Options {
    /// Drop every nth datagram that arrives (0 never).
    pub drop_every: u32,
    /// Answer `sequence number abnormal` until a RESET arrives.
    pub want_reset: bool,
}

fn dir(b: u8, plus: u8, minus: u8) -> f32 {
    if b == plus {
        1.0
    } else if b == minus {
        -1.0
    } else {
        0.0
    }
}

fn drive(p: u8) -> f32 {
    match p & 0xF0 {
        0x20 => ((p & 0x0F) as f32 + 1.0) / 8.0,
        0x30 => -((p & 0x0F) as f32 + 1.0) / 8.0,
        _ => 0.0,
    }
}

fn value(b: &[u8]) -> u16 {
    b.iter().fold(0u16, |v, n| (v << 4) | u16::from(n & 0x0F))
}

fn answer(data: [u8; 4]) -> Vec<Vec<u8>> {
    let mut out = vec![0x90, 0x50];
    out.extend(data);
    out.push(0xFF);
    vec![out]
}

/// The replies to one message: acknowledge and complete, the data of an
/// inquiry, or a syntax error for anything this camera does not know.
pub fn handle(head: &mut Head, m: &[u8]) -> Vec<Vec<u8>> {
    let r = Ranges::default();
    match m {
        [_, 0x01, 0x06, 0x01, vv, ww, x, y, 0xFF] => {
            let pan = dir(*x, 0x02, 0x01) * *vv as f32 / r.pan_speed as f32;
            let tilt = dir(*y, 0x01, 0x02) * *ww as f32 / r.tilt_speed as f32;
            let zoom = head.velocity.zoom;
            head.set_velocity(Velocity { pan, tilt, zoom });
        }
        [_, 0x01, 0x04, 0x07, p, 0xFF] => {
            let v = Velocity { zoom: drive(*p), ..head.velocity };
            head.set_velocity(v);
        }
        [_, 0x01, 0x04, 0x08, p, 0xFF] => head.focus(Focus::Drive(drive(*p))),
        [_, 0x01, 0x04, 0x38, m, 0xFF] => head.focus(if *m == 0x02 { Focus::Auto } else { Focus::Manual }),
        [_, 0x01, 0x04, 0x18, 0x01, 0xFF] => head.focus(Focus::OnePush),
        [_, 0x01, 0x04, 0x3F, 0x01, pp, 0xFF] => head.save(u16::from(*pp) + 1),
        [_, 0x01, 0x04, 0x3F, 0x02, pp, 0xFF] => {
            head.recall(u16::from(*pp) + 1);
        }
        [_, 0x01, 0x06, 0x02, _, _, rest @ .., 0xFF] if rest.len() == 8 => {
            let (p, t) = (value(&rest[..4]) as i16 as f32, value(&rest[4..]) as i16 as f32);
            let tilt = if t >= 0.0 { t / r.tilt_up as f32 } else { t / r.tilt_down as f32 };
            let zoom = head.zoom;
            head.jump(p / r.pan as f32, tilt, zoom);
        }
        [_, 0x01, 0x04, 0x47, a, b, c, d, 0xFF] => {
            let (p, t) = (head.pan, head.tilt);
            head.jump(p, t, value(&[*a, *b, *c, *d]) as f32 / r.zoom as f32);
        }
        [_, 0x01, 0x04, 0x39, p, 0xFF] => {
            head.exposure.mode = Some(match p {
                0x03 => ExposureMode::Manual,
                0x0A => ExposureMode::ShutterPriority,
                0x0B => ExposureMode::IrisPriority,
                _ => ExposureMode::Auto,
            })
        }
        [_, 0x01, 0x04, w @ 0x4A..=0x4C, 0, 0, p, q, 0xFF] => {
            let v = Some(value(&[*p, *q]));
            match w {
                0x4A => head.exposure.shutter = v,
                0x4B => head.exposure.iris = v,
                _ => head.exposure.gain = v,
            }
        }
        [_, 0x09, 0x06, 0x12, 0xFF] => {
            let pos = head.position();
            let tilt = if pos.tilt >= 0.0 { pos.tilt * r.tilt_up as f32 } else { pos.tilt * r.tilt_down as f32 };
            let mut out = answer(nibbles((pos.pan * r.pan as f32).round() as i16 as u16));
            out[0].pop();
            out[0].extend(nibbles(tilt.round() as i16 as u16));
            out[0].push(0xFF);
            return out;
        }
        [_, 0x09, 0x04, 0x47, 0xFF] => return answer(nibbles((head.position().zoom * r.zoom as f32).round() as u16)),
        _ => return vec![vec![0x90, 0x60, 0x02, 0xFF]],
    }
    vec![vec![0x90, 0x41, 0xFF], vec![0x90, 0x51, 0xFF]]
}
