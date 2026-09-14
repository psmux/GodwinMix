//! TSL UMD v5.0 on the wire.
//!
//! Tally lamps do not speak JSON. What they speak, almost without exception,
//! is TSL's Under Monitor Display protocol, and version 5 is the one every
//! current lamp, multiviewer and router understands. It is a small binary
//! format and this file is all of it: about eighty lines of encoder and a
//! decoder that exists so the tests can read back what was written.
//!
//! One packet, as the specification lays it out:
//!
//! ```text
//!  +------+------+-------+--------+   DMSG, repeated:
//!  | PBC  | VER  | FLAGS | SCREEN |   +-------+---------+--------+------+
//!  | u16  | u8   | u8    | u16    |   | INDEX | CONTROL | LENGTH | TEXT |
//!  +------+------+-------+--------+   | u16   | u16     | u16    | ...  |
//!                                     +-------+---------+--------+------+
//! ```
//!
//! Everything is little endian, which is the one thing about TSL that surprises
//! people who have just come from OSC. `PBC` counts every byte after itself.
//! `CONTROL` packs four two bit fields:
//!
//! ```text
//!  bit  0..1  right hand tally      0 off, 1 red, 2 green, 3 amber
//!  bit  2..3  text tally            the colour of the label itself
//!  bit  4..5  left hand tally
//!  bit  6..7  brightness            0 dimmest, 3 brightest
//!  bit    15  control data          0 for a display message, which is all we send
//! ```
//!
//! Over UDP one packet goes in one datagram. Over TCP the packets are written
//! back to back on the stream and `PBC` is what tells the receiver where each
//! one ends, which is why a TCP sender may not omit it.

/// What a lamp shows. The names are the specification's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Lamp {
    #[default]
    Off,
    Red,
    Green,
    Amber,
}

impl Lamp {
    pub fn bits(self) -> u16 {
        match self {
            Lamp::Off => 0,
            Lamp::Red => 1,
            Lamp::Green => 2,
            Lamp::Amber => 3,
        }
    }

    pub fn from_bits(bits: u16) -> Lamp {
        match bits & 0b11 {
            1 => Lamp::Red,
            2 => Lamp::Green,
            3 => Lamp::Amber,
            _ => Lamp::Off,
        }
    }

    /// The names the settings schema offers.
    pub fn parse(name: &str) -> Option<Lamp> {
        match name {
            "off" | "none" => Some(Lamp::Off),
            "red" => Some(Lamp::Red),
            "green" => Some(Lamp::Green),
            "amber" | "yellow" => Some(Lamp::Amber),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Lamp::Off => "off",
            Lamp::Red => "red",
            Lamp::Green => "green",
            Lamp::Amber => "amber",
        }
    }
}

/// One display message: the lamp at `index` and the label beside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    /// The UMD index, which is the number printed on the lamp or set in its
    /// own configuration. 0 based, as the specification has it.
    pub index: u16,
    /// The lamp on the right of the label. Programme goes here by convention.
    pub right: Lamp,
    /// The colour of the label text.
    pub text: Lamp,
    /// The lamp on the left. Preview goes here by convention.
    pub left: Lamp,
    /// 0 dimmest to 3 brightest.
    pub brightness: u8,
    /// What is written on the lamp. Usually the camera's name.
    pub label: String,
}

impl Default for Display {
    fn default() -> Display {
        Display {
            index: 0,
            right: Lamp::Off,
            text: Lamp::Off,
            left: Lamp::Off,
            brightness: 3,
            label: String::new(),
        }
    }
}

impl Display {
    fn control(&self) -> u16 {
        self.right.bits()
            | (self.text.bits() << 2)
            | (self.left.bits() << 4)
            | ((self.brightness as u16 & 0b11) << 6)
    }
}

/// Encode one packet carrying one display message.
///
/// `screen` is the display group the lamps are in; a rack with one tally
/// interface uses 0 and never thinks about it again. `unicode` writes the
/// label as UTF-16LE, which is what a lamp needs for anything outside ASCII;
/// leave it off for an English rig and the packet is half the size.
pub fn encode(screen: u16, display: &Display, unicode: bool) -> Vec<u8> {
    let text: Vec<u8> = if unicode {
        display
            .label
            .encode_utf16()
            .flat_map(|unit| unit.to_le_bytes())
            .collect()
    } else {
        // Anything a lamp cannot show becomes a question mark rather than
        // silently shortening the label and shifting every later field.
        display
            .label
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect()
    };

    let mut body = Vec::with_capacity(8 + text.len());
    body.push(0u8); // VER
    body.push(if unicode { 0x01 } else { 0x00 }); // FLAGS
    body.extend_from_slice(&screen.to_le_bytes());
    body.extend_from_slice(&display.index.to_le_bytes());
    body.extend_from_slice(&display.control().to_le_bytes());
    body.extend_from_slice(&(text.len() as u16).to_le_bytes());
    body.extend_from_slice(&text);

    let mut packet = Vec::with_capacity(2 + body.len());
    packet.extend_from_slice(&(body.len() as u16).to_le_bytes());
    packet.extend_from_slice(&body);
    packet
}

/// What a packet turned out to say. The tests use this, and so does the
/// `test_lamp` tool when it reads its own packet back on a loopback socket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub screen: u16,
    pub unicode: bool,
    pub displays: Vec<Display>,
}

/// Decode one packet. Returns `None` on anything that is not a well formed
/// v5.0 display message, because a tally listener that guesses is worse than
/// one that says nothing.
pub fn decode(packet: &[u8]) -> Option<Packet> {
    if packet.len() < 8 {
        return None;
    }
    let pbc = u16::from_le_bytes([packet[0], packet[1]]) as usize;
    if pbc + 2 > packet.len() {
        return None;
    }
    let body = &packet[2..2 + pbc];
    if body[0] != 0 {
        return None; // not version 5.0
    }
    let unicode = body[1] & 0x01 != 0;
    let screen = u16::from_le_bytes([body[2], body[3]]);

    let mut displays = Vec::new();
    let mut at = 4;
    while at + 6 <= body.len() {
        let index = u16::from_le_bytes([body[at], body[at + 1]]);
        let control = u16::from_le_bytes([body[at + 2], body[at + 3]]);
        at += 4;
        if control & 0x8000 != 0 {
            // A control packet, not a display packet. Nothing here sends one.
            continue;
        }
        let length = u16::from_le_bytes([body[at], body[at + 1]]) as usize;
        at += 2;
        if at + length > body.len() {
            return None;
        }
        let bytes = &body[at..at + length];
        at += length;
        let label = if unicode {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|p| u16::from_le_bytes([p[0], p[1]]))
                .collect();
            String::from_utf16_lossy(&units)
        } else {
            String::from_utf8_lossy(bytes).into_owned()
        };
        displays.push(Display {
            index,
            right: Lamp::from_bits(control),
            text: Lamp::from_bits(control >> 2),
            left: Lamp::from_bits(control >> 4),
            brightness: ((control >> 6) & 0b11) as u8,
            label,
        });
    }
    Some(Packet {
        screen,
        unicode,
        displays,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cam1() -> Display {
        Display {
            index: 3,
            right: Lamp::Red,
            text: Lamp::Red,
            left: Lamp::Off,
            brightness: 3,
            label: "CAM 1".into(),
        }
    }

    #[test]
    fn a_programme_lamp_round_trips() {
        let packet = encode(0, &cam1(), false);
        let read = decode(&packet).expect("a well formed packet");
        assert_eq!(read.screen, 0);
        assert!(!read.unicode);
        assert_eq!(read.displays, vec![cam1()]);
    }

    #[test]
    fn the_byte_count_is_everything_after_itself() {
        let packet = encode(0, &cam1(), false);
        let pbc = u16::from_le_bytes([packet[0], packet[1]]) as usize;
        assert_eq!(pbc, packet.len() - 2);
        // VER 0, FLAGS 0, SCREEN 0, INDEX 3, then CONTROL.
        assert_eq!(&packet[2..8], &[0x00, 0x00, 0x00, 0x00, 0x03, 0x00]);
    }

    #[test]
    fn the_control_word_packs_the_four_fields_where_the_specification_says() {
        let display = Display {
            index: 0,
            right: Lamp::Red,      // 0b01
            text: Lamp::Green,     // 0b10 at bit 2
            left: Lamp::Amber,     // 0b11 at bit 4
            brightness: 3,         // 0b11 at bit 6
            label: String::new(),
        };
        assert_eq!(display.control(), 0b1111_1001);
        let packet = encode(0, &display, false);
        let control = u16::from_le_bytes([packet[8], packet[9]]);
        assert_eq!(control, 0b1111_1001);
        assert_eq!(control & 0x8000, 0, "bit 15 clear means a display message");
    }

    #[test]
    fn every_lamp_colour_survives() {
        for lamp in [Lamp::Off, Lamp::Red, Lamp::Green, Lamp::Amber] {
            let display = Display {
                right: lamp,
                ..Display::default()
            };
            let read = decode(&encode(1, &display, false)).unwrap();
            assert_eq!(read.displays[0].right, lamp, "{}", lamp.name());
        }
    }

    #[test]
    fn a_unicode_label_is_utf16_and_says_so_in_the_flags() {
        let display = Display {
            label: "Bühne".into(),
            ..Display::default()
        };
        let packet = encode(0, &display, true);
        assert_eq!(packet[3] & 0x01, 1, "the unicode flag");
        let read = decode(&packet).unwrap();
        assert!(read.unicode);
        assert_eq!(read.displays[0].label, "Bühne");
    }

    #[test]
    fn an_ascii_packet_replaces_what_a_lamp_cannot_show() {
        let display = Display {
            label: "Bühne".into(),
            ..Display::default()
        };
        let read = decode(&encode(0, &display, false)).unwrap();
        assert_eq!(read.displays[0].label, "B?hne");
        assert_eq!(read.displays[0].label.len(), 5, "the length did not shift");
    }

    #[test]
    fn a_truncated_packet_decodes_to_nothing_rather_than_a_guess() {
        let packet = encode(0, &cam1(), false);
        assert!(decode(&packet[..packet.len() - 3]).is_none());
        assert!(decode(&[]).is_none());
        assert!(decode(&[1, 2, 3]).is_none());
    }

    #[test]
    fn a_packet_that_is_not_version_five_is_refused() {
        let mut packet = encode(0, &cam1(), false);
        packet[2] = 0x01;
        assert!(decode(&packet).is_none());
    }

    #[test]
    fn the_screen_number_is_carried() {
        let read = decode(&encode(7, &cam1(), false)).unwrap();
        assert_eq!(read.screen, 7);
    }

    #[test]
    fn brightness_is_clamped_into_its_two_bits() {
        let display = Display {
            brightness: 9,
            ..Display::default()
        };
        let read = decode(&encode(0, &display, false)).unwrap();
        assert_eq!(read.displays[0].brightness, 1, "9 & 0b11");
    }

    #[test]
    fn a_colour_name_maps_both_ways() {
        for name in ["off", "red", "green", "amber"] {
            assert_eq!(Lamp::parse(name).unwrap().name(), name);
        }
        assert_eq!(Lamp::parse("yellow"), Some(Lamp::Amber));
        assert_eq!(Lamp::parse("puce"), None);
    }
}
