//! The packet header.
//!
//! ```text
//! byte 0..2   flags (top five bits) and the packet length (low eleven bits)
//! byte 2..4   session id
//! byte 4..6   the remote packet id this packet acknowledges
//! byte 6..8   the first packet id to resend, on a resend request
//! byte 8..10  unused, zero
//! byte 10..12 this packet's own id, when it asks for an acknowledgement
//! ```

/// Every packet starts with this many bytes.
pub const HEADER_LEN: usize = 12;

/// Packet ids count up to this and wrap to zero.
pub const MAX_PACKET_ID: u16 = 0x8000;

/// The five flag bits, as they sit after shifting the first byte right by 3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Flags(pub u8);

impl Flags {
    /// The sender wants this packet acknowledged, and it carries an id.
    pub const ACK_REQUEST: Flags = Flags(0x01);
    /// The hello that opens a session, in either direction.
    pub const HELLO: Flags = Flags(0x02);
    /// This packet was sent before and is being sent again.
    pub const RETRANSMIT: Flags = Flags(0x04);
    /// The sender asks for its peer to resend from a packet id.
    pub const RESEND_REQUEST: Flags = Flags(0x08);
    /// This packet acknowledges the id in bytes 4..6.
    pub const ACK_REPLY: Flags = Flags(0x10);

    pub fn has(self, other: Flags) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn with(self, other: Flags) -> Flags {
        Flags(self.0 | other.0)
    }
}

/// A decoded header. `length` is the whole packet, header included.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Header {
    pub flags: Flags,
    pub length: u16,
    pub session: u16,
    pub ack_id: u16,
    pub resend_from: u16,
    pub packet_id: u16,
}

impl Header {
    /// Read a header, or `None` when the datagram is too short or its length
    /// field does not match the datagram, which is how a stray packet from
    /// something that is not an ATEM looks.
    pub fn parse(bytes: &[u8]) -> Option<Header> {
        if bytes.len() < HEADER_LEN {
            return None;
        }
        let word = u16::from_be_bytes([bytes[0], bytes[1]]);
        let length = word & 0x07ff;
        if usize::from(length) != bytes.len() {
            return None;
        }
        let at = |i: usize| u16::from_be_bytes([bytes[i], bytes[i + 1]]);
        Some(Header {
            flags: Flags((word >> 11) as u8),
            length,
            session: at(2),
            ack_id: at(4),
            resend_from: at(6),
            packet_id: at(10),
        })
    }

    /// Write the header into the first twelve bytes of `out`.
    pub fn write(&self, out: &mut [u8]) {
        let word = (u16::from(self.flags.0) << 11) | (self.length & 0x07ff);
        out[0..2].copy_from_slice(&word.to_be_bytes());
        out[2..4].copy_from_slice(&self.session.to_be_bytes());
        out[4..6].copy_from_slice(&self.ack_id.to_be_bytes());
        out[6..8].copy_from_slice(&self.resend_from.to_be_bytes());
        out[8..10].copy_from_slice(&[0, 0]);
        out[10..12].copy_from_slice(&self.packet_id.to_be_bytes());
    }

    /// A whole packet: this header with `payload` after it. The length field
    /// is set from the payload.
    pub fn packet(mut self, payload: &[u8]) -> Vec<u8> {
        let mut out = vec![0u8; HEADER_LEN + payload.len()];
        self.length = out.len() as u16;
        self.write(&mut out);
        out[HEADER_LEN..].copy_from_slice(payload);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_ack_has_the_bytes_the_switcher_expects() {
        let header = Header { flags: Flags::ACK_REPLY, session: 0x8001, ack_id: 7, ..Header::default() };
        assert_eq!(header.packet(&[]), [0x80, 0x0c, 0x80, 0x01, 0, 7, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn a_header_reads_back_what_was_written() {
        let header = Header {
            flags: Flags::ACK_REQUEST.with(Flags::RETRANSMIT),
            session: 0x1234,
            ack_id: 3,
            resend_from: 0,
            packet_id: 0x7fff,
            length: 0,
        };
        let bytes = header.packet(&[1, 2, 3]);
        let back = Header::parse(&bytes).unwrap();
        assert_eq!(back.flags, header.flags);
        assert_eq!(back.length, 15);
        assert_eq!(back.packet_id, 0x7fff);
        assert!(back.flags.has(Flags::RETRANSMIT));
    }

    #[test]
    fn a_length_that_lies_is_refused() {
        let mut bytes = Header::default().packet(&[0; 4]);
        bytes.push(0);
        assert_eq!(Header::parse(&bytes), None);
        assert_eq!(Header::parse(&[0; 5]), None);
    }
}
