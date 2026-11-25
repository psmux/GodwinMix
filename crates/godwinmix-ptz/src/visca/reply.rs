//! VISCA replies: `y0 4z FF` acknowledged, `y0 5z FF` done (with data after
//! `50` for an inquiry), `y0 6z ee FF` an error.

use crate::error::{Code, Error};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reply {
    Ack,
    /// Completion. For an inquiry the bytes between `y0 50` and `FF`.
    Done(Vec<u8>),
    Error(u8),
}

/// One message, `y0` through `FF`. `None` for anything that is not a reply.
pub fn parse(m: &[u8]) -> Option<Reply> {
    if m.len() < 3 || m[0] & 0x8F != 0x80 || m[0] == 0x80 || *m.last()? != 0xFF {
        return None;
    }
    match m[1] & 0xF0 {
        0x40 => Some(Reply::Ack),
        0x50 => Some(Reply::Done(m[2..m.len() - 1].to_vec())),
        0x60 if m.len() >= 4 => Some(Reply::Error(m[2])),
        _ => None,
    }
}

/// What an error byte means, and what to do about it.
pub fn error(to: &str, code: u8) -> Error {
    let what = match code {
        0x01 => "says the message was too long",
        0x02 => "did not understand the command (syntax error). This camera may not have this control",
        0x03 => "is busy (command buffer full). Send fewer commands at once",
        0x04 => "cancelled the command",
        0x05 => "had no socket for the cancel",
        0x41 => "cannot do that now (command not executable). It may be in a menu, powered down or in the wrong mode",
        _ => "refused the command",
    };
    Error::new(Code::Refused, format!("{to} {what} (VISCA error {code:#04x})."))
}

/// A 16 bit value from four bytes of one nibble each.
pub fn from_nibbles(b: &[u8]) -> Option<u16> {
    (b.len() >= 4).then(|| b[..4].iter().fold(0u16, |v, n| (v << 4) | u16::from(n & 0x0F)))
}

/// Split a byte stream into messages at each `FF`. Answers the messages and
/// whatever is left over after the last one.
pub fn split(buf: &[u8]) -> (Vec<Vec<u8>>, Vec<u8>) {
    let mut out = Vec::new();
    let mut start = 0;
    for (i, b) in buf.iter().enumerate() {
        if *b == 0xFF {
            out.push(buf[start..=i].to_vec());
            start = i + 1;
        }
    }
    (out, buf[start..].to_vec())
}
