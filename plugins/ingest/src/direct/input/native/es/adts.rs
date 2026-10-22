//! AAC in ADTS to the bodies `aacparse` and the tagger make: the
//! AudioSpecificConfig as a sequence header whenever it changes, then one
//! raw frame per tag, its ADTS header taken off.

use super::frames::{Framer, Split};
use super::h264::header;
use super::rate::samples_ns;
use super::{Parser, Pes, Said, Unit};
use crate::media_tag::TagKind;
use crate::tagger::prefix;

const RATES: [u32; 13] = [96_000, 88_200, 64_000, 48_000, 44_100, 32_000, 24_000, 22_050, 16_000, 12_000, 11_025, 8_000, 7_350];

#[derive(Default)]
pub struct Adts {
    framer: Framer,
    sent: Option<[u8; 2]>,
    said: Said,
}

/// One ADTS header, read.
struct Head {
    len: usize,
    header_len: usize,
    config: [u8; 2],
    rate: u32,
    channels: u32,
}

fn head(f: &[u8]) -> Option<Head> {
    if f.len() < 7 || f[0] != 0xFF || f[1] & 0xF6 != 0xF0 {
        return None;
    }
    let profile = u16::from(f[2] >> 6);
    let index = (f[2] >> 2) & 0x0F;
    let channels = ((f[2] & 1) << 2) | (f[3] >> 6);
    let len = (usize::from(f[3] & 3) << 11) | (usize::from(f[4]) << 3) | usize::from(f[5] >> 5);
    let header_len = if f[1] & 1 == 1 { 7 } else { 9 };
    let rate = *RATES.get(usize::from(index))?;
    let config = (((profile + 1) << 11) | (u16::from(index) << 7) | (u16::from(channels) << 3)).to_be_bytes();
    (len > header_len).then_some(Head { len, header_len, config, rate, channels: u32::from(channels) })
}

impl Split for Adts {
    fn frame_len(&self, f: &[u8]) -> Option<usize> {
        head(f).map(|h| h.len)
    }
}

impl Parser for Adts {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let mut frames = Vec::new();
        let mut framer = std::mem::take(&mut self.framer);
        framer.push(self, pes, &mut frames);
        self.framer = framer;
        for (stamped, f) in frames {
            let Some(h) = head(&f) else { continue };
            let Some(at) = self.framer.clock.frame(stamped, samples_ns(1024, h.rate)) else { continue };
            if self.sent != Some(h.config) {
                out.push(header(at, &prefix(TagKind::Audio, false, true, 0), &h.config));
                self.sent = Some(h.config);
                self.said = Said { codec: "aac", channels: h.channels, sample_rate: h.rate };
            }
            let mut body = prefix(TagKind::Audio, false, false, 0);
            body.extend_from_slice(&f[h.header_len..h.len]);
            out.push(Unit { at, body, keyframe: false, header: false });
        }
    }

    fn said(&self) -> Said {
        self.said.clone()
    }
}
