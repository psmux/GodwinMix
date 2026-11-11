//! Sony's VISCA over IP framing: an eight byte header in front of every
//! VISCA message on UDP port 52381. Two bytes of payload type, two of
//! payload length and four of sequence number, all big endian.

pub const PORT: u16 = 52381;

/// The payload types in the header.
pub const COMMAND: u16 = 0x0100;
pub const INQUIRY: u16 = 0x0110;
pub const REPLY: u16 = 0x0111;
pub const CONTROL: u16 = 0x0200;
pub const CONTROL_REPLY: u16 = 0x0201;

/// The control payload that resets the camera's sequence number to 0.
pub const RESET: [u8; 1] = [0x01];
/// Control replies that say the sequence number or the message was wrong.
pub const SEQ_ABNORMAL: [u8; 2] = [0x0F, 0x01];
pub const MSG_ABNORMAL: [u8; 2] = [0x0F, 0x02];

pub fn wrap(kind: u16, seq: u32, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(8 + payload.len());
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(&(payload.len() as u16).to_be_bytes());
    out.extend_from_slice(&seq.to_be_bytes());
    out.extend_from_slice(payload);
    out
}

/// The header's type, sequence number and payload, or `None` when the
/// datagram is not a whole VISCA over IP message.
pub fn unwrap(datagram: &[u8]) -> Option<(u16, u32, &[u8])> {
    if datagram.len() < 8 {
        return None;
    }
    let kind = u16::from_be_bytes([datagram[0], datagram[1]]);
    let len = u16::from_be_bytes([datagram[2], datagram[3]]) as usize;
    let seq = u32::from_be_bytes([datagram[4], datagram[5], datagram[6], datagram[7]]);
    let payload = datagram.get(8..8 + len)?;
    Some((kind, seq, payload))
}
