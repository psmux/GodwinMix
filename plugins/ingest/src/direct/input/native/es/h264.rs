//! H.264 in Annex B to the bodies `h264parse` and the tagger make: an
//! `avcC` sequence header whenever the parameter sets change, then each
//! access unit with four byte lengths in front of its NAL units.

use super::annexb::{push_sized, units};
use super::{cts_ms, Parser, Pes, Said, Unit};
use crate::media_tag::TagKind;
use crate::tagger::prefix;

const IDR: u8 = 5;
const SPS: u8 = 7;
const PPS: u8 = 8;

#[derive(Default)]
pub struct H264 {
    sps: Option<Vec<u8>>,
    pps: Option<Vec<u8>>,
    /// The `avcC` last sent as a header.
    sent: Option<Vec<u8>>,
}

impl Parser for H264 {
    fn pes(&mut self, pes: &Pes, out: &mut Vec<Unit>) {
        let Some(at) = pes.at() else { return };
        let mut au = Vec::with_capacity(pes.data.len() + 16);
        let mut keyframe = false;
        for nal in units(pes.data) {
            match nal[0] & 0x1F {
                SPS => self.sps = Some(nal.to_vec()),
                PPS => self.pps = Some(nal.to_vec()),
                IDR => keyframe = true,
                _ => {}
            }
            push_sized(&mut au, nal);
        }
        // `h264parse` hands on nothing before it has the parameter sets.
        let Some(config) = avcc(self.sps.as_deref(), self.pps.as_deref()) else { return };
        if au.is_empty() {
            return;
        }
        if self.sent.as_ref() != Some(&config) {
            out.push(header(at, &prefix(TagKind::Video, true, true, 0), &config));
            self.sent = Some(config);
        }
        let mut body = prefix(TagKind::Video, keyframe, false, cts_ms(pes));
        body.extend_from_slice(&au);
        out.push(Unit { at, body, keyframe, header: false });
    }

    fn said(&self) -> Said {
        Said { codec: "h264", ..Said::default() }
    }
}

/// A sequence header unit: `head`, then the configuration record.
pub fn header(at: u64, head: &[u8], config: &[u8]) -> Unit {
    let mut body = head.to_vec();
    body.extend_from_slice(config);
    Unit { at, body, keyframe: true, header: true }
}

/// The AVCDecoderConfigurationRecord (ISO 14496-15, 5.2.4.1) for one SPS
/// and one PPS, four byte lengths, as `h264parse` writes it: with the
/// chroma format and bit depths after the PPS for the high profiles, and no
/// SPS extensions.
pub fn avcc(sps: Option<&[u8]>, pps: Option<&[u8]>) -> Option<Vec<u8>> {
    let (sps, pps) = (sps?, pps?);
    if sps.len() < 4 {
        return None;
    }
    let mut c = vec![1, sps[1], sps[2], sps[3], 0xFF, 0xE1];
    c.extend_from_slice(&(sps.len() as u16).to_be_bytes());
    c.extend_from_slice(sps);
    c.push(1);
    c.extend_from_slice(&(pps.len() as u16).to_be_bytes());
    c.extend_from_slice(pps);
    if let Some((chroma, luma, chroma_depth)) = high(sps) {
        c.extend_from_slice(&[0xFC | chroma, 0xF8 | luma, 0xF8 | chroma_depth, 0]);
    }
    Some(c)
}

/// The chroma format and the two bit depths less eight, for a profile whose
/// SPS carries them (7.3.2.1.1).
fn high(sps: &[u8]) -> Option<(u8, u8, u8)> {
    if !matches!(sps[1], 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135) {
        return None;
    }
    let mut b = crate::sps::Bits::new(sps.get(4..)?);
    b.ue()?;
    let chroma = b.ue()?;
    if chroma == 3 {
        b.bit()?;
    }
    Some((chroma as u8, b.ue()? as u8, b.ue()? as u8))
}
