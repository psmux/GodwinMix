//! PAT, PMT and SDT: which programs there are, what is in each, what each is
//! called. Read from whole sections, and the two the filter rewrites built
//! again from scratch.

use serde_json::{json, Value};

#[path = "tables_build.rs"]
mod build;
pub use build::{build_pat, build_pmt};
#[path = "tables_sdt.rs"]
mod sdt;
pub use sdt::parse_sdt;

/// One elementary stream, as the PMT lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stream {
    pub pid: u16,
    pub stream_type: u8,
    /// The descriptors as they came, so a rebuilt PMT loses nothing.
    pub info: Vec<u8>,
}

impl Stream {
    /// What a person calls it: `h264`, `aac`, `ac3`, `teletext`.
    pub fn kind(&self) -> &'static str {
        match self.stream_type {
            0x01 | 0x02 => "mpeg2 video",
            0x1B => "h264",
            0x24 => "hevc",
            0x03 | 0x04 => "mpeg audio",
            0x0F => "aac",
            0x11 => "aac latm",
            0x81 => "ac3",
            0x87 => "eac3",
            0x06 => self.private_kind(),
            _ => "data",
        }
    }

    /// Stream type 6 is "private data" and the descriptors say what it is.
    fn private_kind(&self) -> &'static str {
        for (tag, _) in descriptors(&self.info) {
            match tag {
                0x6A => return "ac3",
                0x7A => return "eac3",
                0x56 => return "teletext",
                0x59 => return "subtitles",
                _ => {}
            }
        }
        "data"
    }

    /// The ISO 639 language, when a descriptor gives one.
    pub fn language(&self) -> Option<String> {
        descriptors(&self.info)
            .find(|(tag, body)| *tag == 0x0A && body.len() >= 3)
            .map(|(_, body)| String::from_utf8_lossy(&body[..3]).into_owned())
    }

    pub fn json(&self) -> Value {
        json!({"pid": self.pid, "kind": self.kind(), "language": self.language()})
    }
}

/// One program, as far as the tables have told us.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Program {
    pub number: u16,
    pub pmt_pid: u16,
    pub pcr_pid: u16,
    pub info: Vec<u8>,
    pub streams: Vec<Stream>,
    /// From the SDT's service descriptor, when the feed carries one.
    pub name: String,
    pub provider: String,
}

impl Program {
    pub fn label(&self) -> String {
        if self.name.is_empty() {
            format!("{}", self.number)
        } else {
            format!("{} ({})", self.number, self.name)
        }
    }

    pub fn json(&self) -> Value {
        json!({
            "program": self.number, "name": self.name, "provider": self.provider,
            "pmt_pid": self.pmt_pid, "pcr_pid": self.pcr_pid,
            "streams": self.streams.iter().map(Stream::json).collect::<Vec<_>>(),
        })
    }
}

/// Walk a descriptor loop: `(tag, body)` pairs, stopping at a short one.
pub fn descriptors(mut d: &[u8]) -> impl Iterator<Item = (u8, &[u8])> {
    std::iter::from_fn(move || {
        if d.len() < 2 || d.len() < 2 + d[1] as usize {
            return None;
        }
        let (tag, len) = (d[0], d[1] as usize);
        let body = &d[2..2 + len];
        d = &d[2 + len..];
        Some((tag, body))
    })
}

/// The loop bytes of a long form section: after the 8 byte header, before
/// the CRC.
pub(super) fn body(section: &[u8]) -> &[u8] {
    &section[8.min(section.len())..section.len().saturating_sub(4)]
}

fn pid_at(b: &[u8]) -> u16 {
    (u16::from(b[0] & 0x1F) << 8) | u16::from(b[1])
}

/// `(program number, PMT PID)` for every program in a PAT. Program 0 is the
/// NIT and is left out.
pub fn parse_pat(section: &[u8]) -> Vec<(u16, u16)> {
    body(section)
        .chunks_exact(4)
        .map(|c| (u16::from_be_bytes([c[0], c[1]]), pid_at(&c[2..])))
        .filter(|(n, _)| *n != 0)
        .collect()
}

/// A PMT's PCR PID, program descriptors and streams.
pub fn parse_pmt(section: &[u8]) -> Option<(u16, Vec<u8>, Vec<Stream>)> {
    let b = body(section);
    if b.len() < 4 {
        return None;
    }
    let pcr = pid_at(b);
    let info_len = (usize::from(b[2] & 0x0F) << 8) | usize::from(b[3]);
    let info = b.get(4..4 + info_len)?.to_vec();
    let mut rest = &b[4 + info_len..];
    let mut streams = Vec::new();
    while rest.len() >= 5 {
        let es_len = (usize::from(rest[3] & 0x0F) << 8) | usize::from(rest[4]);
        let Some(es_info) = rest.get(5..5 + es_len) else { break };
        streams.push(Stream { pid: pid_at(&rest[1..]), stream_type: rest[0], info: es_info.to_vec() });
        rest = &rest[5 + es_len..];
    }
    Some((pcr, info, streams))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_built_pat_parses_back_to_what_went_in() {
        let pat = build_pat(1, 3, 42, 0x1000);
        assert_eq!(crate::ts::crc32(&pat), 0);
        assert_eq!(parse_pat(&pat), vec![(42, 0x1000)]);
    }

    #[test]
    fn a_rebuilt_pmt_keeps_descriptors_and_drops_what_was_not_chosen() {
        let lang = vec![0x0A, 4, b'e', b'n', b'g', 0];
        let video = Stream { pid: 0x100, stream_type: 0x1B, info: vec![] };
        let audio = Stream { pid: 0x101, stream_type: 0x06, info: [vec![0x6A, 0], lang].concat() };
        let p = Program { number: 7, pmt_pid: 0x1000, pcr_pid: 0x100, ..Default::default() };
        let (pcr, _, streams) = parse_pmt(&build_pmt(&p, 0, &[&video, &audio])).unwrap();
        assert_eq!(pcr, 0x100);
        assert_eq!(streams[1].kind(), "ac3");
        assert_eq!(streams[1].language().as_deref(), Some("eng"));
        let (_, _, only) = parse_pmt(&build_pmt(&p, 0, &[&video])).unwrap();
        assert_eq!(only, vec![video]);
    }
}
