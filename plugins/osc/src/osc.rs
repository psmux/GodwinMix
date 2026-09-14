//! OSC 1.0 on the wire, written here rather than pulled in.
//!
//! The whole of OSC that a control surface uses is four argument types and a
//! bundle, and the encoder and decoder for that fit on two screens. The `rosc`
//! crate is good and it is also 2,000 lines plus `nom`; this file is under 300
//! including its tests, and it compiles in a fraction of a second on a Pi.
//!
//! What is here:
//!
//! * addresses and type tag strings, four byte aligned and null padded;
//! * arguments `i` (int32), `f` (float32), `s` (string), `b` (blob), and the
//!   tagless booleans `T`, `F` and `N` that TouchOSC and Open Stage Control
//!   send for a button;
//! * `#bundle`, decoded by flattening the elements in order. The time tag is
//!   read and ignored: a mixer executes a take now or not at all, and a
//!   surface that wanted a scheduled take should say `at_running_time_ms`.
//!
//! Everything is big endian, which is what the specification says and what
//! every implementation does.

use std::fmt;

/// One OSC argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    Str(String),
    Blob(Vec<u8>),
    Bool(bool),
    Nil,
}

impl Arg {
    /// The number a control surface meant, whatever type it sent it as.
    ///
    /// A fader sends a float, a button sends `1`, TouchOSC sends `T`. They all
    /// mean the same thing to `/source/<id>/audio/mute`.
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Arg::Int(i) => Some(*i as f64),
            Arg::Float(f) => Some(*f as f64),
            Arg::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
            Arg::Str(s) => s.parse::<f64>().ok(),
            _ => None,
        }
    }

    /// The string a control surface meant. A take carries a source id.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Arg::Str(s) => Some(s),
            _ => None,
        }
    }

    /// True when the argument reads as "pressed". A momentary button sends 1
    /// on the way down and 0 on the way up, and acting on both fires twice.
    pub fn is_truthy(&self) -> bool {
        self.as_f64().map(|n| n != 0.0).unwrap_or(false)
    }
}

/// One OSC message: an address and its arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub address: String,
    pub args: Vec<Arg>,
}

impl Message {
    pub fn new(address: impl Into<String>, args: Vec<Arg>) -> Message {
        Message {
            address: address.into(),
            args,
        }
    }

    /// The address split on `/`, with the leading empty piece dropped.
    pub fn parts(&self) -> Vec<&str> {
        self.address.split('/').filter(|p| !p.is_empty()).collect()
    }

    /// Encode this message as an OSC packet.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(32 + self.args.len() * 8);
        put_str(&mut out, &self.address);
        let mut tags = String::from(",");
        for arg in &self.args {
            tags.push(match arg {
                Arg::Int(_) => 'i',
                Arg::Float(_) => 'f',
                Arg::Str(_) => 's',
                Arg::Blob(_) => 'b',
                Arg::Bool(true) => 'T',
                Arg::Bool(false) => 'F',
                Arg::Nil => 'N',
            });
        }
        put_str(&mut out, &tags);
        for arg in &self.args {
            match arg {
                Arg::Int(i) => out.extend_from_slice(&i.to_be_bytes()),
                Arg::Float(f) => out.extend_from_slice(&f.to_be_bytes()),
                Arg::Str(s) => put_str(&mut out, s),
                Arg::Blob(b) => {
                    out.extend_from_slice(&(b.len() as i32).to_be_bytes());
                    out.extend_from_slice(b);
                    pad(&mut out);
                }
                // T, F and N carry no bytes: the tag is the value.
                Arg::Bool(_) | Arg::Nil => {}
            }
        }
        out
    }
}

/// What went wrong reading a packet. Every variant names the byte offset,
/// because a surface that sends malformed OSC sends it every tick and the
/// offset is the only thing that tells you which field it got wrong.
#[derive(Debug, Clone, PartialEq)]
pub struct DecodeError {
    pub at: usize,
    pub why: String,
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "bad OSC packet at byte {}: {}", self.at, self.why)
    }
}

impl std::error::Error for DecodeError {}

fn err(at: usize, why: impl Into<String>) -> DecodeError {
    DecodeError {
        at,
        why: why.into(),
    }
}

/// Decode a packet into the messages it carries. A bundle flattens; a bare
/// message answers with one element.
pub fn decode(packet: &[u8]) -> Result<Vec<Message>, DecodeError> {
    let mut out = Vec::new();
    decode_into(packet, 0, &mut out, 0)?;
    Ok(out)
}

fn decode_into(
    packet: &[u8],
    base: usize,
    out: &mut Vec<Message>,
    depth: usize,
) -> Result<(), DecodeError> {
    if depth > 4 {
        return Err(err(base, "bundles nested more than four deep"));
    }
    if packet.starts_with(b"#bundle\0") {
        // 8 bytes of "#bundle\0", 8 of time tag, then size prefixed elements.
        let mut at = 16;
        if packet.len() < at {
            return Err(err(base, "a bundle header shorter than sixteen bytes"));
        }
        while at < packet.len() {
            let size = read_i32(packet, at).ok_or_else(|| err(base + at, "a truncated element size"))?;
            at += 4;
            let size = usize::try_from(size).map_err(|_| err(base + at, "a negative element size"))?;
            let end = at
                .checked_add(size)
                .filter(|e| *e <= packet.len())
                .ok_or_else(|| err(base + at, "an element that runs past the end of the bundle"))?;
            decode_into(&packet[at..end], base + at, out, depth + 1)?;
            at = end;
        }
        return Ok(());
    }
    out.push(decode_message(packet, base)?);
    Ok(())
}

fn decode_message(packet: &[u8], base: usize) -> Result<Message, DecodeError> {
    let (address, mut at) = take_str(packet, 0).map_err(|e| err(base + e.at, e.why))?;
    if !address.starts_with('/') {
        return Err(err(base, format!("an address that does not start with '/': {address:?}")));
    }
    let mut args = Vec::new();
    if at >= packet.len() {
        // No type tag string at all. Legal in OSC 1.0 and sent by some older
        // surfaces; it means no arguments.
        return Ok(Message { address, args });
    }
    let (tags, next) = take_str(packet, at).map_err(|e| err(base + e.at, e.why))?;
    at = next;
    if !tags.starts_with(',') {
        return Err(err(base, format!("a type tag string without a comma: {tags:?}")));
    }
    for tag in tags.chars().skip(1) {
        match tag {
            'i' => {
                let v = read_i32(packet, at).ok_or_else(|| err(base + at, "a truncated int32"))?;
                at += 4;
                args.push(Arg::Int(v));
            }
            'f' => {
                let v = read_i32(packet, at).ok_or_else(|| err(base + at, "a truncated float32"))?;
                at += 4;
                args.push(Arg::Float(f32::from_bits(v as u32)));
            }
            's' | 'S' => {
                let (s, next) = take_str(packet, at).map_err(|e| err(base + e.at, e.why))?;
                at = next;
                args.push(Arg::Str(s));
            }
            'b' => {
                let len = read_i32(packet, at).ok_or_else(|| err(base + at, "a truncated blob size"))?;
                at += 4;
                let len = usize::try_from(len).map_err(|_| err(base + at, "a negative blob size"))?;
                let end = at
                    .checked_add(len)
                    .filter(|e| *e <= packet.len())
                    .ok_or_else(|| err(base + at, "a blob that runs past the end"))?;
                args.push(Arg::Blob(packet[at..end].to_vec()));
                at = (end + 3) & !3;
            }
            'T' => args.push(Arg::Bool(true)),
            'F' => args.push(Arg::Bool(false)),
            'N' => args.push(Arg::Nil),
            'I' => args.push(Arg::Float(f32::INFINITY)),
            // A type this build does not know. Skipping is not safe, because
            // every later argument is at an unknown offset, so stop here and
            // keep what was read.
            other => {
                return Err(err(
                    base + at,
                    format!("type tag '{other}' is not one this build reads (i, f, s, b, T, F, N)"),
                ))
            }
        }
    }
    Ok(Message { address, args })
}

fn read_i32(packet: &[u8], at: usize) -> Option<i32> {
    let bytes: [u8; 4] = packet.get(at..at + 4)?.try_into().ok()?;
    Some(i32::from_be_bytes(bytes))
}

/// A null terminated string, padded to the next four byte boundary.
fn take_str(packet: &[u8], at: usize) -> Result<(String, usize), DecodeError> {
    let end = packet[at.min(packet.len())..]
        .iter()
        .position(|b| *b == 0)
        .map(|p| at + p)
        .ok_or_else(|| err(at, "a string with no null terminator"))?;
    let text = std::str::from_utf8(&packet[at..end])
        .map_err(|_| err(at, "a string that is not UTF-8"))?
        .to_string();
    Ok((text, (end + 4) & !3))
}

fn put_str(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    pad(out);
}

fn pad(out: &mut Vec<u8>) {
    while out.len() % 4 != 0 {
        out.push(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_take_round_trips() {
        let message = Message::new("/program/take", vec![Arg::Str("cam1".into())]);
        let bytes = message.encode();
        assert_eq!(bytes.len() % 4, 0, "every OSC packet is four byte aligned");
        assert_eq!(decode(&bytes).unwrap(), vec![message]);
    }

    #[test]
    fn the_wire_bytes_are_what_the_specification_says() {
        // "/a\0\0" ",i\0\0" then 42 big endian.
        let bytes = Message::new("/a", vec![Arg::Int(42)]).encode();
        assert_eq!(bytes, b"/a\0\0,i\0\0\0\0\0\x2a");
    }

    #[test]
    fn every_argument_type_survives_a_round_trip() {
        let message = Message::new(
            "/all",
            vec![
                Arg::Int(-7),
                Arg::Float(-6.0),
                Arg::Str("hello".into()),
                Arg::Blob(vec![1, 2, 3]),
                Arg::Bool(true),
                Arg::Bool(false),
                Arg::Nil,
            ],
        );
        assert_eq!(decode(&message.encode()).unwrap(), vec![message]);
    }

    #[test]
    fn a_bundle_flattens_in_order() {
        let one = Message::new("/program/take", vec![Arg::Str("cam1".into())]);
        let two = Message::new("/source/cam1/audio/gain", vec![Arg::Float(-6.0)]);
        let mut packet = Vec::from(&b"#bundle\0"[..]);
        packet.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]); // time tag, ignored
        for message in [&one, &two] {
            let body = message.encode();
            packet.extend_from_slice(&(body.len() as i32).to_be_bytes());
            packet.extend_from_slice(&body);
        }
        assert_eq!(decode(&packet).unwrap(), vec![one, two]);
    }

    #[test]
    fn a_message_with_no_type_tags_is_read_as_no_arguments() {
        let decoded = decode(b"/program/revert\0").unwrap();
        assert_eq!(decoded, vec![Message::new("/program/revert", vec![])]);
    }

    #[test]
    fn a_truncated_packet_names_the_byte() {
        let error = decode(b"/a\0\0,i\0\0\0\0").unwrap_err();
        assert_eq!(error.at, 8);
        assert!(error.why.contains("int32"), "{error}");
    }

    #[test]
    fn an_address_without_a_slash_is_refused() {
        assert!(decode(b"a\0\0\0,\0\0\0").is_err());
    }

    #[test]
    fn a_button_reads_as_a_number_whatever_it_sent() {
        assert_eq!(Arg::Bool(true).as_f64(), Some(1.0));
        assert_eq!(Arg::Int(1).as_f64(), Some(1.0));
        assert_eq!(Arg::Float(0.0).as_f64(), Some(0.0));
        assert!(Arg::Int(1).is_truthy());
        assert!(!Arg::Bool(false).is_truthy());
    }

    #[test]
    fn parts_drops_the_empty_leading_piece() {
        let message = Message::new("/source/cam1/audio/gain", vec![]);
        assert_eq!(message.parts(), vec!["source", "cam1", "audio", "gain"]);
    }

    #[test]
    fn an_unknown_type_tag_is_an_error_rather_than_a_silent_misread() {
        // 'h' is a 64 bit int, which this build does not read. Skipping it
        // would put every later argument at the wrong offset.
        let error = decode(b"/a\0\0,h\0\0\0\0\0\0\0\0\0\x01").unwrap_err();
        assert!(error.why.contains("'h'"), "{error}");
    }

    #[test]
    fn a_bundle_element_that_overruns_is_refused() {
        let mut packet = Vec::from(&b"#bundle\0"[..]);
        packet.extend_from_slice(&[0; 8]);
        packet.extend_from_slice(&1000i32.to_be_bytes());
        packet.extend_from_slice(b"/a\0\0");
        assert!(decode(&packet).is_err());
    }
}
